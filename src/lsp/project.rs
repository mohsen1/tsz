//! Project container for multi-file LSP operations.
//!
//! This provides a lightweight home for parsed files, binders, and line maps so
//! LSP features can be extended across multiple files.

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::binder::BinderState;
use crate::binder::SymbolId;
use crate::checker::TypeCache;
use crate::checker::state::CheckerState;
#[cfg(not(target_arch = "wasm32"))]
use crate::cli::config::{load_tsconfig, resolve_compiler_options};
use crate::lsp::code_actions::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidate,
    ImportCandidateKind,
};
use crate::lsp::completions::{CompletionItem, CompletionItemKind, Completions};
use crate::lsp::definition::GoToDefinition;
use crate::lsp::diagnostics::{LspDiagnostic, convert_diagnostic};
use crate::lsp::hover::{HoverInfo, HoverProvider};
use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::references::FindReferences;
use crate::lsp::rename::{RenameProvider, TextEdit, WorkspaceEdit};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats};
use crate::lsp::signature_help::{SignatureHelp, SignatureHelpProvider};
use crate::lsp::utils::find_node_at_offset;
use crate::parser::ParserState;
use crate::parser::node::NodeAccess;
use crate::parser::{NodeIndex, NodeList, node::NodeArena, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::solver::TypeInterner;

enum ImportKind {
    Named(String),
    Default,
    Namespace,
}

struct ImportTarget {
    module_specifier: String,
    kind: ImportKind,
}

struct NamespaceReexportTarget {
    file: String,
    namespace: String,
    member: String,
}

struct ExportMatch {
    kind: ImportCandidateKind,
    is_type_only: bool,
}

struct ImportSpecifierTarget {
    local_ident: NodeIndex,
    property_name: Option<NodeIndex>,
}

struct IncrementalUpdatePlan {
    reparse_start: u32,
    prefix_nodes: Vec<NodeIndex>,
}

const INCREMENTAL_NODE_MULTIPLIER: usize = 4;
const INCREMENTAL_MIN_NODE_BUDGET: usize = 4096;

/// Parsed file state used by LSP features.
pub struct ProjectFile {
    file_name: String,
    root: NodeIndex,
    parser: ParserState,
    binder: BinderState,
    line_map: LineMap,
    type_interner: TypeInterner,
    type_cache: Option<TypeCache>,
    scope_cache: ScopeCache,
    strict: bool,
}

impl ProjectFile {
    /// Parse and bind a single source file for LSP queries.
    pub fn new(file_name: String, source_text: String) -> Self {
        Self::with_strict(file_name, source_text, false)
    }

    /// Parse and bind a single source file with explicit strict mode setting.
    pub fn with_strict(file_name: String, source_text: String, strict: bool) -> Self {
        let mut parser = ParserState::new(file_name.clone(), source_text);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(parser.get_source_text());

        Self {
            file_name,
            root,
            parser,
            binder,
            line_map,
            type_interner: TypeInterner::new(),
            type_cache: None,
            scope_cache: ScopeCache::default(),
            strict,
        }
    }

    /// File name used for LSP locations.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Root node of the parsed source file.
    pub fn root(&self) -> NodeIndex {
        self.root
    }

    /// Arena containing parsed Nodes.
    pub fn arena(&self) -> &NodeArena {
        self.parser.get_arena()
    }

    /// Binder state for symbol lookup.
    pub fn binder(&self) -> &BinderState {
        &self.binder
    }

    /// Line map for offset <-> position conversions.
    pub fn line_map(&self) -> &LineMap {
        &self.line_map
    }

    /// Original source text for this file.
    pub fn source_text(&self) -> &str {
        self.parser.get_source_text()
    }

    /// Get the strict mode setting for type checking.
    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Set the strict mode for type checking.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    pub fn update_source(&mut self, source_text: String) {
        self.parser.reset(self.file_name.clone(), source_text);
        self.root = self.parser.parse_source_file();

        let arena = self.parser.get_arena();
        self.binder.reset();
        self.binder.bind_source_file(arena, self.root);

        self.line_map = LineMap::build(self.parser.get_source_text());
        self.type_cache = None;
        self.scope_cache.clear();
    }

    pub fn update_source_with_edits(&mut self, source_text: String, edits: &[TextEdit]) {
        if edits.is_empty() {
            self.update_source(source_text);
            return;
        }

        if let Some(plan) = self.incremental_update_plan(edits, source_text.len()) {
            if self.apply_incremental_update(source_text, plan) {
                return;
            }
            let refreshed = self.parser.get_source_text().to_string();
            self.update_source(refreshed);
            return;
        }

        self.update_source(source_text);
    }

    fn incremental_update_plan(
        &self,
        edits: &[TextEdit],
        new_text_len: usize,
    ) -> Option<IncrementalUpdatePlan> {
        let (change_start, _) = self.change_range_from_edits(edits)?;
        if change_start == 0 {
            return None;
        }

        let arena = self.parser.get_arena();
        let root_node = arena.get(self.root)?;
        let source_file = arena.get_source_file(root_node)?;
        let mut reparse_start = change_start;

        for &stmt_idx in &source_file.statements.nodes {
            let stmt = arena.get(stmt_idx)?;
            if change_start < stmt.end {
                if change_start >= stmt.pos {
                    reparse_start = stmt.pos;
                }
                break;
            }
        }

        if reparse_start == 0 {
            return None;
        }

        let estimated_nodes = (new_text_len / 20).max(1);
        let max_nodes = estimated_nodes
            .saturating_mul(INCREMENTAL_NODE_MULTIPLIER)
            .max(INCREMENTAL_MIN_NODE_BUDGET);
        if arena.len() > max_nodes {
            return None;
        }

        let mut prefix_nodes = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let stmt = arena.get(stmt_idx)?;
            if stmt.pos < reparse_start {
                prefix_nodes.push(stmt_idx);
            } else {
                break;
            }
        }

        Some(IncrementalUpdatePlan {
            reparse_start,
            prefix_nodes,
        })
    }

    fn change_range_from_edits(&self, edits: &[TextEdit]) -> Option<(u32, u32)> {
        let source_text = self.parser.get_source_text();
        let mut min_start: Option<u32> = None;
        let mut max_end: Option<u32> = None;

        for edit in edits {
            let start = self
                .line_map
                .position_to_offset(edit.range.start, source_text)?;
            let end = self
                .line_map
                .position_to_offset(edit.range.end, source_text)?;
            min_start = Some(min_start.map_or(start, |current| current.min(start)));
            max_end = Some(max_end.map_or(end, |current| current.max(end)));
        }

        Some((min_start?, max_end?))
    }

    fn apply_incremental_update(
        &mut self,
        source_text: String,
        plan: IncrementalUpdatePlan,
    ) -> bool {
        let old_suffix_nodes = {
            let arena = self.parser.get_arena();
            let Some(root_node) = arena.get(self.root) else {
                return false;
            };
            let Some(source_file) = arena.get_source_file(root_node) else {
                return false;
            };
            let prefix_len = plan.prefix_nodes.len();
            if prefix_len > source_file.statements.nodes.len() {
                return false;
            }
            source_file.statements.nodes[prefix_len..].to_vec()
        };

        let parse_result = self.parser.parse_source_file_statements_from_offset(
            self.file_name.clone(),
            source_text,
            plan.reparse_start,
        );
        if parse_result.reparse_start != plan.reparse_start {
            return false;
        }

        let new_text = self.parser.get_source_text().to_string();
        let line_map = LineMap::build(&new_text);
        let comments = crate::comments::get_comment_ranges(&new_text);

        let mut combined_nodes =
            Vec::with_capacity(plan.prefix_nodes.len() + parse_result.statements.nodes.len());
        combined_nodes.extend(plan.prefix_nodes.iter().copied());
        combined_nodes.extend(parse_result.statements.nodes.iter().copied());

        let new_statements = NodeList {
            nodes: combined_nodes,
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        };

        let root = self.root;
        {
            let arena = &mut self.parser.arena;
            for &node in &parse_result.statements.nodes {
                if let Some(ext) = arena.get_extended_mut(node) {
                    ext.parent = root;
                }
            }
            if let Some(ext) = arena.get_extended_mut(parse_result.end_of_file_token) {
                ext.parent = root;
            }
            if let Some(root_node) = arena.get_mut(root) {
                root_node.end = parse_result.end_pos;
            }
            let Some(root_node) = arena.get(root) else {
                return false;
            };
            let data_index = root_node.data_index as usize;
            let Some(source_file) = arena.source_files.get_mut(data_index) else {
                return false;
            };

            source_file.statements = new_statements;
            source_file.end_of_file_token = parse_result.end_of_file_token;
            source_file.text = std::sync::Arc::from(new_text.into_boxed_str());
            source_file.comments = comments;
        }

        self.line_map = line_map;
        let arena = self.parser.get_arena();
        if !self.binder.bind_source_file_incremental(
            arena,
            self.root,
            &plan.prefix_nodes,
            &old_suffix_nodes,
            &parse_result.statements.nodes,
            plan.reparse_start,
        ) {
            self.binder.reset();
            self.binder.bind_source_file(arena, self.root);
        }
        self.type_cache = None;
        self.scope_cache.clear();

        true
    }

    pub fn get_hover(&mut self, position: Position) -> Option<HoverInfo> {
        self.get_hover_with_stats(position, None)
    }

    pub fn get_hover_with_stats(
        &mut self,
        position: Position,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        let provider = HoverProvider::with_strict(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.strict,
        );

        provider.get_hover_with_scope_cache(
            self.root,
            position,
            &mut self.type_cache,
            &mut self.scope_cache,
            scope_stats,
        )
    }

    pub fn get_signature_help(&mut self, position: Position) -> Option<SignatureHelp> {
        self.get_signature_help_with_stats(position, None)
    }

    pub fn get_signature_help_with_stats(
        &mut self,
        position: Position,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SignatureHelp> {
        let provider = SignatureHelpProvider::with_strict(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.strict,
        );

        provider.get_signature_help_with_scope_cache(
            self.root,
            position,
            &mut self.type_cache,
            &mut self.scope_cache,
            scope_stats,
        )
    }

    pub fn get_completions(&mut self, position: Position) -> Option<Vec<CompletionItem>> {
        self.get_completions_with_stats(position, None)
    }

    pub fn get_completions_with_stats(
        &mut self,
        position: Position,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<Vec<CompletionItem>> {
        let provider = Completions::with_strict(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.strict,
        );

        provider.get_completions_with_caches(
            self.root,
            position,
            &mut self.type_cache,
            &mut self.scope_cache,
            scope_stats,
        )
    }

    pub fn get_diagnostics(&mut self) -> Vec<LspDiagnostic> {
        let file_name = self.file_name.clone();
        let source_text = self.parser.get_source_text();
        let compiler_options = crate::checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };

        let mut checker = if let Some(cache) = self.type_cache.take() {
            CheckerState::with_cache(
                self.parser.get_arena(),
                &self.binder,
                &self.type_interner,
                file_name,
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.parser.get_arena(),
                &self.binder,
                &self.type_interner,
                file_name,
                compiler_options,
            )
        };

        checker.check_source_file(self.root);

        let diagnostics = checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| convert_diagnostic(diag, &self.line_map, source_text))
            .collect();

        self.type_cache = Some(checker.extract_cache());
        diagnostics
    }

    fn node_location(&self, node_idx: NodeIndex) -> Option<Location> {
        let node = self.arena().get(node_idx)?;
        let start = self
            .line_map
            .offset_to_position(node.pos, self.source_text());
        let end = self
            .line_map
            .offset_to_position(node.end, self.source_text());
        Some(Location {
            file_path: self.file_name.clone(),
            range: Range::new(start, end),
        })
    }

    fn resolve_symbol(&self, node_idx: NodeIndex) -> Option<SymbolId> {
        if node_idx.is_none() {
            return None;
        }

        if let Some(&sym_id) = self.binder.node_symbols.get(&node_idx.0) {
            return Some(sym_id);
        }

        self.binder.resolve_identifier(self.arena(), node_idx)
    }

    fn export_locations(&self, export_name: &str) -> Vec<Location> {
        self.export_nodes(export_name)
            .into_iter()
            .filter_map(|node| self.node_location(node))
            .collect()
    }

    fn export_nodes(&self, export_name: &str) -> Vec<NodeIndex> {
        let arena = self.arena();
        let binder = self.binder();
        let mut nodes = Vec::new();

        let Some(root_node) = arena.get(self.root()) else {
            return Vec::new();
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return Vec::new();
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if !export.module_specifier.is_none() {
                continue;
            }

            if export.is_default_export {
                if export_name == "default" {
                    self.push_default_export_nodes(export.export_clause, &mut nodes);
                }
                continue;
            }

            if export_name == "default" || export.export_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                self.push_named_export_nodes(export.export_clause, export_name, &mut nodes);
                continue;
            }

            if !self.declaration_has_name(export.export_clause, export_name) {
                continue;
            }

            if let Some(sym_id) = binder.file_locals.get(export_name) {
                self.push_symbol_decls(sym_id, &mut nodes);
            } else {
                nodes.push(export.export_clause);
            }
        }

        nodes.sort_by_key(|node| node.0);
        nodes.dedup();
        nodes
    }

    fn exported_names_for_symbol(&self, sym_id: SymbolId) -> Vec<String> {
        let mut names = Vec::new();
        let arena = self.arena();
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            return names;
        };
        let local_name = symbol.escaped_name.as_str();

        let Some(root_node) = arena.get(self.root()) else {
            return names;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return names;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };
            if !export.module_specifier.is_none() {
                continue;
            }

            if export.is_default_export {
                if !export.export_clause.is_none()
                    && self.resolve_symbol(export.export_clause) == Some(sym_id)
                {
                    names.push("default".to_string());
                }
                continue;
            }

            if export.export_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                if let Some(named) = arena.get_named_imports(clause_node) {
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec_node) = arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(spec) = arena.get_specifier(spec_node) else {
                            continue;
                        };

                        let local_ident = if !spec.property_name.is_none() {
                            spec.property_name
                        } else {
                            spec.name
                        };
                        if self.resolve_symbol(local_ident) != Some(sym_id) {
                            continue;
                        }

                        let export_ident = if !spec.name.is_none() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if let Some(export_text) = arena.get_identifier_text(export_ident) {
                            names.push(export_text.to_string());
                        }
                    }
                }
                continue;
            }

            if self.declaration_has_name(export.export_clause, local_name) {
                names.push(local_name.to_string());
            }
        }

        names.sort();
        names.dedup();
        names
    }

    fn import_targets_for_local(&self, local_name: &str) -> Vec<ImportTarget> {
        let mut targets = Vec::new();
        let arena = self.arena();

        let Some(root_node) = arena.get(self.root()) else {
            return targets;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return targets;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }
            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let module_specifier = module_specifier.to_string();

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if !clause.name.is_none()
                && let Some(name) = arena.get_identifier_text(clause.name)
                && name == local_name
            {
                targets.push(ImportTarget {
                    module_specifier: module_specifier.clone(),
                    kind: ImportKind::Default,
                });
            }

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                if let Some(name) = arena.get_identifier_text(clause.named_bindings)
                    && name == local_name
                {
                    targets.push(ImportTarget {
                        module_specifier: module_specifier.clone(),
                        kind: ImportKind::Namespace,
                    });
                }
                continue;
            }
            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            if !named.name.is_none()
                && let Some(name) = arena.get_identifier_text(named.name)
                && name == local_name
            {
                targets.push(ImportTarget {
                    module_specifier: module_specifier.clone(),
                    kind: ImportKind::Namespace,
                });
            }

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let local_ident = if !spec.name.is_none() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(local_text) = arena.get_identifier_text(local_ident) else {
                    continue;
                };
                if local_text != local_name {
                    continue;
                }

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = arena.get_identifier_text(export_ident) else {
                    continue;
                };

                targets.push(ImportTarget {
                    module_specifier: module_specifier.clone(),
                    kind: ImportKind::Named(export_text.to_string()),
                });
            }
        }

        targets
    }

    fn push_default_export_nodes(&self, clause_idx: NodeIndex, nodes: &mut Vec<NodeIndex>) {
        if clause_idx.is_none() {
            return;
        }

        if let Some(&sym_id) = self.binder.node_symbols.get(&clause_idx.0) {
            self.push_symbol_decls(sym_id, nodes);
            return;
        }

        if let Some(sym_id) = self.binder.resolve_identifier(self.arena(), clause_idx) {
            self.push_symbol_decls(sym_id, nodes);
            return;
        }

        nodes.push(clause_idx);
    }

    fn push_named_export_nodes(
        &self,
        clause_idx: NodeIndex,
        export_name: &str,
        nodes: &mut Vec<NodeIndex>,
    ) {
        let arena = self.arena();
        let binder = self.binder();

        let Some(clause_node) = arena.get(clause_idx) else {
            return;
        };
        let Some(named) = arena.get_named_imports(clause_node) else {
            return;
        };

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = arena.get(spec_idx) else {
                continue;
            };
            let Some(spec) = arena.get_specifier(spec_node) else {
                continue;
            };

            let export_ident = if !spec.name.is_none() {
                spec.name
            } else {
                spec.property_name
            };
            let Some(export_text) = arena.get_identifier_text(export_ident) else {
                continue;
            };
            if export_text != export_name {
                continue;
            }

            let local_ident = if !spec.property_name.is_none() {
                spec.property_name
            } else {
                spec.name
            };
            if let Some(local_text) = arena.get_identifier_text(local_ident) {
                if let Some(sym_id) = binder.file_locals.get(local_text) {
                    self.push_symbol_decls(sym_id, nodes);
                } else {
                    nodes.push(spec_idx);
                }
            }
        }
    }

    fn push_symbol_decls(&self, sym_id: SymbolId, nodes: &mut Vec<NodeIndex>) {
        if let Some(symbol) = self.binder.symbols.get(sym_id) {
            nodes.extend(symbol.declarations.iter().copied());
        }
    }

    fn declaration_has_name(&self, decl_idx: NodeIndex, export_name: &str) -> bool {
        let arena = self.arena();
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                arena
                    .get_function(node)
                    .and_then(|func| arena.get_identifier_text(func.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                arena
                    .get_class(node)
                    .and_then(|class| arena.get_identifier_text(class.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                arena
                    .get_interface(node)
                    .and_then(|iface| arena.get_identifier_text(iface.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                arena
                    .get_type_alias(node)
                    .and_then(|alias| arena.get_identifier_text(alias.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                arena
                    .get_enum(node)
                    .and_then(|enm| arena.get_identifier_text(enm.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                arena
                    .get_module(node)
                    .and_then(|module| arena.get_identifier_text(module.name))
                    == Some(export_name)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT
                || k == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                || k == syntax_kind_ext::VARIABLE_DECLARATION =>
            {
                let mut decls = Vec::new();
                self.collect_variable_declarations(decl_idx, &mut decls);
                decls.into_iter().any(|decl_idx| {
                    let Some(decl_node) = arena.get(decl_idx) else {
                        return false;
                    };
                    arena
                        .get_variable_declaration(decl_node)
                        .and_then(|decl| arena.get_identifier_text(decl.name))
                        == Some(export_name)
                })
            }
            _ => false,
        }
    }

    fn collect_variable_declarations(&self, node_idx: NodeIndex, output: &mut Vec<NodeIndex>) {
        let arena = self.arena();
        let Some(node) = arena.get(node_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            output.push(node_idx);
            return;
        }

        if (node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            || node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST)
            && let Some(var) = arena.get_variable(node)
        {
            for &child in &var.declarations.nodes {
                self.collect_variable_declarations(child, output);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectRequestKind {
    Definition,
    References,
    Rename,
    Hover,
    SignatureHelp,
    Completions,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectRequestTiming {
    pub duration: Duration,
    pub scope_hits: u32,
    pub scope_misses: u32,
}

#[derive(Debug, Default, Clone)]
pub struct ProjectPerformance {
    timings: FxHashMap<ProjectRequestKind, ProjectRequestTiming>,
}

impl ProjectPerformance {
    fn record(&mut self, kind: ProjectRequestKind, duration: Duration, stats: ScopeCacheStats) {
        let timing = ProjectRequestTiming {
            duration,
            scope_hits: stats.hits,
            scope_misses: stats.misses,
        };
        self.timings.insert(kind, timing);
    }

    pub fn timing(&self, kind: ProjectRequestKind) -> Option<ProjectRequestTiming> {
        self.timings.get(&kind).copied()
    }
}

fn apply_text_edits(source: &str, line_map: &LineMap, edits: &[TextEdit]) -> Option<String> {
    let mut edits_with_offsets = Vec::with_capacity(edits.len());
    for edit in edits {
        let start = line_map.position_to_offset(edit.range.start, source)? as usize;
        let end = line_map.position_to_offset(edit.range.end, source)? as usize;
        if start > end || end > source.len() {
            return None;
        }
        edits_with_offsets.push((start, end, edit));
    }

    edits_with_offsets.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

    let mut result = source.to_string();
    for (start, end, edit) in edits_with_offsets {
        result.replace_range(start..end, &edit.new_text);
    }

    Some(result)
}

/// Multi-file container for LSP operations.
pub struct Project {
    files: FxHashMap<String, ProjectFile>,
    performance: ProjectPerformance,
    strict: bool,
}

impl Project {
    /// Create a new empty project.
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
            performance: ProjectPerformance::default(),
            strict: false,
        }
    }

    /// Get the strict mode setting for type checking.
    pub fn strict(&self) -> bool {
        self.strict
    }

    /// Load TypeScript configuration from a tsconfig.json file.
    /// This updates the project's strict mode based on the compiler options.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_tsconfig(&mut self, workspace_root: &Path) -> Result<(), String> {
        let tsconfig_path = workspace_root.join("tsconfig.json");
        match load_tsconfig(&tsconfig_path) {
            Ok(config) => {
                let resolved = resolve_compiler_options(config.compiler_options.as_ref())
                    .map_err(|e| format!("failed to resolve compiler options: {}", e))?;
                self.strict = resolved.checker.strict;
                // Update strict mode on all existing files
                for file in self.files.values_mut() {
                    file.set_strict(self.strict);
                }
                Ok(())
            }
            Err(_) => {
                // If tsconfig is not found or fails to parse, keep default (false)
                Ok(())
            }
        }
    }

    /// Set the strict mode directly.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        // Update strict mode on all existing files
        for file in self.files.values_mut() {
            file.set_strict(strict);
        }
    }

    /// Total number of files tracked by the project.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Snapshot of per-request timing data.
    pub fn performance(&self) -> &ProjectPerformance {
        &self.performance
    }

    /// Add or replace a file, re-parsing and re-binding its contents.
    pub fn set_file(&mut self, file_name: String, source_text: String) {
        let file = ProjectFile::with_strict(file_name.clone(), source_text, self.strict);
        self.files.insert(file_name, file);
    }

    /// Update an existing file by applying incremental text edits.
    pub fn update_file(&mut self, file_name: &str, edits: &[TextEdit]) -> Option<()> {
        if edits.is_empty() {
            return Some(());
        }

        let (updated_source, unchanged) = {
            let file = self.files.get(file_name)?;
            let source = file.source_text();
            let updated = apply_text_edits(source, file.line_map(), edits)?;
            let unchanged = updated == source;
            (updated, unchanged)
        };

        if unchanged {
            return Some(());
        }

        let file = self.files.get_mut(file_name)?;
        file.update_source_with_edits(updated_source, edits);
        Some(())
    }

    /// Remove a file from the project.
    pub fn remove_file(&mut self, file_name: &str) -> Option<ProjectFile> {
        self.files.remove(file_name)
    }

    /// Fetch a file by name.
    pub fn file(&self, file_name: &str) -> Option<&ProjectFile> {
        self.files.get(file_name)
    }

    /// Go to definition within a single file.
    pub fn get_definition(&mut self, file_name: &str, position: Position) -> Option<Vec<Location>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            {
                let file = self.files.get(file_name)?;
                if let Some(definitions) = self.definition_from_import(file, position) {
                    return Some(definitions);
                }
            }

            let file = self.files.get_mut(file_name)?;
            let arena = file.parser.get_arena();
            let binder = &file.binder;
            let line_map = &file.line_map;
            let source_text = file.parser.get_source_text();
            let file_name = file.file_name.clone();
            let root = file.root;
            let goto_def = GoToDefinition::new(arena, binder, line_map, file_name, source_text);
            goto_def.get_definition_with_scope_cache(
                root,
                position,
                &mut file.scope_cache,
                Some(&mut scope_stats),
            )
        })();

        self.performance
            .record(ProjectRequestKind::Definition, start.elapsed(), scope_stats);

        result
    }

    /// Hover within a single file.
    pub fn get_hover(&mut self, file_name: &str, position: Position) -> Option<HoverInfo> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = self
            .files
            .get_mut(file_name)?
            .get_hover_with_stats(position, Some(&mut scope_stats));

        self.performance
            .record(ProjectRequestKind::Hover, start.elapsed(), scope_stats);

        result
    }

    /// Signature help within a single file.
    pub fn get_signature_help(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<SignatureHelp> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = self
            .files
            .get_mut(file_name)?
            .get_signature_help_with_stats(position, Some(&mut scope_stats));

        self.performance.record(
            ProjectRequestKind::SignatureHelp,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Completions within a single file.
    pub fn get_completions(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<CompletionItem>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let mut completions = {
            let file = self.files.get_mut(file_name)?;
            file.get_completions_with_stats(position, Some(&mut scope_stats))
                .unwrap_or_default()
        };

        let mut existing = FxHashSet::default();
        for item in &completions {
            existing.insert(item.label.clone());
        }

        let (missing_name, skip_auto_import) = {
            let file = self.files.get(file_name)?;
            if let Some((node_idx, name)) = self.identifier_at_position(file, position) {
                let skip = self.is_member_access_node(file.arena(), node_idx);
                (Some(name), skip)
            } else {
                (None, false)
            }
        };

        if let Some(missing_name) = missing_name
            && !skip_auto_import
            && !existing.contains(&missing_name)
        {
            let file = self.files.get(file_name)?;
            let mut candidates = Vec::new();
            let mut seen = FxHashSet::default();
            self.collect_import_candidates_for_name(
                file,
                &missing_name,
                &mut candidates,
                &mut seen,
            );

            for candidate in candidates {
                if existing.contains(&candidate.local_name) {
                    continue;
                }
                completions.push(self.completion_from_import_candidate(&candidate));
            }
        }

        let result = if completions.is_empty() {
            None
        } else {
            completions.sort_by(|a, b| a.label.cmp(&b.label));
            Some(completions)
        };

        self.performance.record(
            ProjectRequestKind::Completions,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Diagnostics within a single file.
    pub fn get_diagnostics(&mut self, file_name: &str) -> Option<Vec<LspDiagnostic>> {
        let start = Instant::now();
        let scope_stats = ScopeCacheStats::default();
        let result = {
            let file = self.files.get_mut(file_name)?;
            Some(file.get_diagnostics())
        };

        self.performance.record(
            ProjectRequestKind::Diagnostics,
            start.elapsed(),
            scope_stats,
        );

        result
    }

    /// Code actions for a file (project-aware).
    pub fn get_code_actions(
        &self,
        file_name: &str,
        range: Range,
        diagnostics: Vec<LspDiagnostic>,
        only: Option<Vec<CodeActionKind>>,
    ) -> Option<Vec<CodeAction>> {
        let file = self.files.get(file_name)?;
        let import_candidates = self.import_candidates_for_diagnostics(file, &diagnostics);

        let provider = CodeActionProvider::new(
            file.arena(),
            file.binder(),
            file.line_map(),
            file.file_name().to_string(),
            file.source_text(),
        );

        let actions = provider.provide_code_actions(
            file.root(),
            range,
            CodeActionContext {
                diagnostics,
                only,
                import_candidates,
            },
        );

        if actions.is_empty() {
            None
        } else {
            Some(actions)
        }
    }

    fn collect_file_references(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        scope_stats: Option<&mut ScopeCacheStats>,
        output: &mut Vec<Location>,
    ) {
        if node_idx.is_none() {
            return;
        }

        let find_refs = FindReferences::new(
            file.parser.get_arena(),
            &file.binder,
            &file.line_map,
            file.file_name.clone(),
            file.parser.get_source_text(),
        );

        if let Some(mut refs) = find_refs.find_references_for_node_with_scope_cache(
            file.root(),
            node_idx,
            &mut file.scope_cache,
            scope_stats,
        ) {
            output.append(&mut refs);
        }
    }

    fn collect_file_rename_edits(
        file: &mut ProjectFile,
        node_idx: NodeIndex,
        new_name: &str,
        output: &mut WorkspaceEdit,
    ) {
        let mut locations = Vec::new();
        Self::collect_file_references(file, node_idx, None, &mut locations);
        for location in locations {
            output.add_edit(
                location.file_path,
                TextEdit::new(location.range, new_name.to_string()),
            );
        }
    }

    fn dedup_workspace_edit(workspace_edit: &mut WorkspaceEdit) {
        for edits in workspace_edit.changes.values_mut() {
            let mut seen = FxHashSet::default();
            edits.retain(|edit| {
                let key = (
                    edit.range.start.line,
                    edit.range.start.character,
                    edit.range.end.line,
                    edit.range.end.character,
                );
                seen.insert(key)
            });
        }
    }

    fn import_binding_nodes(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<NodeIndex> {
        let mut bindings = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return bindings;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return bindings;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if export_name == "default" && !clause.name.is_none() {
                bindings.push(clause.name);
            }

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(imported_name) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if imported_name != export_name {
                    continue;
                }

                bindings.push(spec_idx);
            }
        }

        bindings
    }

    fn import_specifier_targets_for_export(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<ImportSpecifierTarget> {
        let mut targets = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return targets;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return targets;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if !spec.name.is_none() {
                    spec.name
                } else {
                    spec.property_name
                };
                let property_name = if !spec.property_name.is_none() {
                    Some(spec.property_name)
                } else {
                    None
                };

                targets.push(ImportSpecifierTarget {
                    local_ident,
                    property_name,
                });
            }
        }

        targets
    }

    fn named_import_local_names(
        &self,
        file: &ProjectFile,
        target_file: &str,
        export_name: &str,
    ) -> Vec<String> {
        let mut locals = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return locals;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return locals;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(named) = arena.get_named_imports(bindings_node) else {
                continue;
            };

            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = arena.get_specifier(spec_node) else {
                    continue;
                };

                let export_ident = if !spec.property_name.is_none() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = arena.get_identifier_text(export_ident) else {
                    continue;
                };
                if export_text != export_name {
                    continue;
                }

                let local_ident = if !spec.name.is_none() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(local_text) = arena.get_identifier_text(local_ident) else {
                    continue;
                };
                locals.push(local_text.to_string());
            }
        }

        locals
    }

    fn reexport_targets_for(
        &self,
        source_file: &str,
        export_name: &str,
        refs: &mut Vec<Location>,
    ) -> (Vec<(String, String)>, Vec<NamespaceReexportTarget>) {
        let mut targets = Vec::new();
        let mut namespace_targets = Vec::new();

        for (file_name, file) in &self.files {
            let arena = file.arena();
            let Some(root_node) = arena.get(file.root()) else {
                continue;
            };
            let Some(source_file_node) = arena.get_source_file(root_node) else {
                continue;
            };

            for &stmt_idx in &source_file_node.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                    continue;
                }

                let Some(export) = arena.get_export_decl(stmt_node) else {
                    continue;
                };
                if export.module_specifier.is_none() {
                    continue;
                }

                let Some(module_specifier) = arena.get_literal_text(export.module_specifier) else {
                    continue;
                };
                let Some(resolved) =
                    self.resolve_module_specifier(file.file_name(), module_specifier)
                else {
                    continue;
                };
                if resolved != source_file {
                    continue;
                }

                if export.export_clause.is_none() {
                    if export_name != "default" {
                        targets.push((file_name.clone(), export_name.to_string()));
                    }
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                    if clause_node.kind == SyntaxKind::Identifier as u16
                        && let Some(ns_name) = arena.get_identifier_text(export.export_clause)
                    {
                        namespace_targets.push(NamespaceReexportTarget {
                            file: file_name.clone(),
                            namespace: ns_name.to_string(),
                            member: export_name.to_string(),
                        });
                    }
                    continue;
                }

                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };

                    let import_ident = if !spec.property_name.is_none() {
                        spec.property_name
                    } else {
                        spec.name
                    };
                    let Some(import_text) = arena.get_identifier_text(import_ident) else {
                        continue;
                    };
                    if import_text != export_name {
                        continue;
                    }

                    if let Some(location) = file.node_location(import_ident) {
                        refs.push(location);
                    }

                    let export_ident = if !spec.name.is_none() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    if let Some(export_text) = arena.get_identifier_text(export_ident) {
                        targets.push((file_name.clone(), export_text.to_string()));
                    }
                }
            }
        }

        (targets, namespace_targets)
    }

    fn namespace_import_names(&self, file: &ProjectFile, target_file: &str) -> Vec<String> {
        let mut names = Vec::new();
        let arena = file.arena();

        let Some(root_node) = arena.get(file.root()) else {
            return names;
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return names;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION
                && stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                continue;
            }

            let Some(import) = arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_specifier) = arena.get_literal_text(import.module_specifier) else {
                continue;
            };
            let Some(resolved) = self.resolve_module_specifier(file.file_name(), module_specifier)
            else {
                continue;
            };
            if resolved != target_file {
                continue;
            }

            if import.import_clause.is_none() {
                continue;
            }

            let Some(clause_node) = arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.named_bindings.is_none() {
                continue;
            }

            let Some(bindings_node) = arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind != syntax_kind_ext::NAMESPACE_IMPORT {
                continue;
            }

            let Some(bindings) = arena.get_named_imports(bindings_node) else {
                continue;
            };
            if let Some(name) = arena.get_identifier_text(bindings.name) {
                names.push(name.to_string());
            }
        }

        names
    }

    fn collect_namespace_member_locations(
        &self,
        file: &ProjectFile,
        namespace_name: &str,
        export_name: &str,
        output: &mut Vec<Location>,
    ) {
        let arena = file.arena();
        let expected_symbol = file.binder().file_locals.get(namespace_name);

        for node in arena.nodes.iter() {
            if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                continue;
            }

            let Some(access) = arena.get_access_expr(node) else {
                continue;
            };
            let expr_idx = access.expression;
            let Some(expr_node) = arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }

            let Some(expr_text) = arena.get_identifier_text(expr_idx) else {
                continue;
            };
            if expr_text != namespace_name {
                continue;
            }

            if let Some(sym_id) = expected_symbol
                && file.binder().resolve_identifier(arena, expr_idx) != Some(sym_id)
            {
                continue;
            }

            let member_idx = access.name_or_argument;
            let matches = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                arena.get_identifier_text(member_idx) == Some(export_name)
            } else {
                arena.get_literal_text(member_idx) == Some(export_name)
            };

            if !matches {
                continue;
            }

            if let Some(location) = file.node_location(member_idx) {
                output.push(location);
            }
        }
    }

    /// Find references within a single file.
    pub fn find_references(
        &mut self,
        file_name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let (node_idx, symbol_id, local_name) = {
                let file = self.files.get_mut(file_name)?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.parser.get_source_text())?;
                let node_idx = find_node_at_offset(file.parser.get_arena(), offset);
                if node_idx.is_none() {
                    return None;
                }

                let finder = FindReferences::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                let symbol_id = finder.resolve_symbol_for_node_with_scope_cache(
                    file.root(),
                    node_idx,
                    &mut file.scope_cache,
                    Some(&mut scope_stats),
                )?;
                let symbol = file.binder().symbols.get(symbol_id)?;
                let local_name = symbol.escaped_name.clone();
                (node_idx, symbol_id, local_name)
            };

            let mut locations = Vec::new();
            {
                let file = self.files.get_mut(file_name)?;
                Self::collect_file_references(
                    file,
                    node_idx,
                    Some(&mut scope_stats),
                    &mut locations,
                );
            }

            let (import_targets, export_names, source_file_name) = {
                let file = self.files.get(file_name)?;
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = if import_targets.is_empty() {
                    file.exported_names_for_symbol(symbol_id)
                } else {
                    Vec::new()
                };
                (import_targets, export_names, file.file_name().to_string())
            };

            let mut cross_targets: Vec<(String, String)> = Vec::new();
            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };
                    match target.kind {
                        ImportKind::Named(name) => cross_targets.push((resolved, name)),
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()))
                        }
                        ImportKind::Namespace => {}
                    }
                }
            } else {
                for export_name in export_names {
                    cross_targets.push((source_file_name.clone(), export_name));
                }
            }

            let mut expanded_targets = Vec::new();
            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }
                expanded_targets.push((def_file.clone(), export_name.clone()));

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                locations.extend(reexport_refs);
                pending.extend(reexports);
                namespace_targets.extend(reexport_namespaces);
            }

            let file_names: Vec<String> = self.files.keys().cloned().collect();

            for (def_file, export_name) in expanded_targets {
                let export_nodes = {
                    let target_file = self.files.get(&def_file);
                    target_file
                        .map(|file| file.export_nodes(&export_name))
                        .unwrap_or_default()
                };
                if !export_nodes.is_empty()
                    && let Some(target_file) = self.files.get_mut(&def_file)
                {
                    for node in export_nodes {
                        Self::collect_file_references(
                            target_file,
                            node,
                            Some(&mut scope_stats),
                            &mut locations,
                        );
                    }
                }

                for other_name in &file_names {
                    if other_name == &def_file {
                        continue;
                    }

                    let binding_nodes = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.import_binding_nodes(file, &def_file, &export_name))
                            .unwrap_or_default()
                    };
                    if !binding_nodes.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for node in binding_nodes {
                            Self::collect_file_references(
                                other_file,
                                node,
                                Some(&mut scope_stats),
                                &mut locations,
                            );
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                for other_name in &file_names {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                    }
                }
            }

            if locations.is_empty() {
                return None;
            }

            locations.sort_by(|a, b| {
                let file_cmp = a.file_path.cmp(&b.file_path);
                if file_cmp != Ordering::Equal {
                    return file_cmp;
                }
                let start_cmp = (a.range.start.line, a.range.start.character)
                    .cmp(&(b.range.start.line, b.range.start.character));
                if start_cmp != Ordering::Equal {
                    return start_cmp;
                }
                (a.range.end.line, a.range.end.character)
                    .cmp(&(b.range.end.line, b.range.end.character))
            });
            locations.dedup_by(|a, b| a.file_path == b.file_path && a.range == b.range);

            Some(locations)
        })();

        self.performance
            .record(ProjectRequestKind::References, start.elapsed(), scope_stats);

        result
    }

    /// Rename a symbol across files in the project.
    pub fn get_rename_edits(
        &mut self,
        file_name: &str,
        position: Position,
        new_name: String,
    ) -> Result<WorkspaceEdit, String> {
        let start = Instant::now();
        let mut scope_stats = ScopeCacheStats::default();
        let result = (|| {
            let normalized_name = {
                let file = self
                    .files
                    .get(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let provider = RenameProvider::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                provider.normalize_rename_at_position(position, &new_name)?
            };

            let (symbol_id, local_name, import_targets, export_names, source_file_name) = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let offset = file
                    .line_map
                    .position_to_offset(position, file.source_text())
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let node_idx = find_node_at_offset(file.arena(), offset);
                if node_idx.is_none() {
                    return Err("Could not find symbol to rename".to_string());
                }

                let finder = FindReferences::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                let symbol_id = finder
                    .resolve_symbol_for_node_with_scope_cache(
                        file.root(),
                        node_idx,
                        &mut file.scope_cache,
                        Some(&mut scope_stats),
                    )
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let symbol = file
                    .binder()
                    .symbols
                    .get(symbol_id)
                    .ok_or_else(|| "Could not find symbol to rename".to_string())?;
                let local_name = symbol.escaped_name.clone();
                let import_targets = file.import_targets_for_local(&local_name);
                let export_names = file.exported_names_for_symbol(symbol_id);

                (
                    symbol_id,
                    local_name,
                    import_targets,
                    export_names,
                    file.file_name().to_string(),
                )
            };

            let mut workspace_edit = {
                let file = self
                    .files
                    .get_mut(file_name)
                    .ok_or_else(|| "You cannot rename this element.".to_string())?;
                let provider = RenameProvider::new(
                    file.parser.get_arena(),
                    &file.binder,
                    &file.line_map,
                    file.file_name.clone(),
                    file.parser.get_source_text(),
                );
                provider.provide_rename_edits_for_symbol(
                    file.root(),
                    symbol_id,
                    normalized_name.clone(),
                )?
            };

            let mut cross_targets = Vec::new();

            if !import_targets.is_empty() {
                for target in import_targets {
                    let Some(resolved) =
                        self.resolve_module_specifier(&source_file_name, &target.module_specifier)
                    else {
                        continue;
                    };

                    match target.kind {
                        ImportKind::Named(name) => {
                            if name == local_name {
                                cross_targets.push((resolved, name));
                            }
                        }
                        ImportKind::Default => {
                            cross_targets.push((resolved, "default".to_string()));
                        }
                        ImportKind::Namespace => {}
                    }
                }
            }

            let mut export_names: Vec<String> = export_names
                .into_iter()
                .filter(|name| name == &local_name)
                .collect();
            export_names.sort();
            export_names.dedup();

            for export_name in export_names {
                cross_targets.push((source_file_name.clone(), export_name));
            }

            if cross_targets.is_empty() {
                Self::dedup_workspace_edit(&mut workspace_edit);
                return Ok(workspace_edit);
            }

            let file_names: Vec<String> = self.files.keys().cloned().collect();
            let mut pending = cross_targets;
            let mut seen_targets: FxHashSet<(String, String)> = FxHashSet::default();
            let mut namespace_targets = Vec::new();

            while let Some((def_file, export_name)) = pending.pop() {
                if !seen_targets.insert((def_file.clone(), export_name.clone())) {
                    continue;
                }

                if def_file != file_name {
                    let export_nodes = {
                        let target_file = self.files.get(&def_file);
                        target_file
                            .map(|file| file.export_nodes(&export_name))
                            .unwrap_or_default()
                    };
                    if !export_nodes.is_empty()
                        && let Some(target_file) = self.files.get_mut(&def_file)
                    {
                        for node in export_nodes {
                            Self::collect_file_rename_edits(
                                target_file,
                                node,
                                &normalized_name,
                                &mut workspace_edit,
                            );
                        }
                    }
                }

                let mut reexport_refs = Vec::new();
                let (reexports, reexport_namespaces) =
                    self.reexport_targets_for(&def_file, &export_name, &mut reexport_refs);
                for location in reexport_refs {
                    workspace_edit.add_edit(
                        location.file_path,
                        TextEdit::new(location.range, normalized_name.clone()),
                    );
                }

                for (reexport_file, reexport_name) in reexports {
                    if reexport_name == export_name {
                        pending.push((reexport_file, reexport_name));
                    }
                }

                namespace_targets.extend(reexport_namespaces);

                for other_name in &file_names {
                    if other_name == &def_file {
                        continue;
                    }

                    let import_targets = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.import_specifier_targets_for_export(
                                    file,
                                    &def_file,
                                    &export_name,
                                )
                            })
                            .unwrap_or_default()
                    };
                    if !import_targets.is_empty()
                        && let Some(other_file) = self.files.get_mut(other_name)
                    {
                        for target in import_targets {
                            if let Some(property_name) = target.property_name {
                                if let Some(location) = other_file.node_location(property_name) {
                                    workspace_edit.add_edit(
                                        location.file_path,
                                        TextEdit::new(location.range, normalized_name.clone()),
                                    );
                                }
                            } else {
                                if other_name == file_name {
                                    continue;
                                }
                                Self::collect_file_rename_edits(
                                    other_file,
                                    target.local_ident,
                                    &normalized_name,
                                    &mut workspace_edit,
                                );
                            }
                        }
                    }

                    let namespace_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| self.namespace_import_names(file, &def_file))
                            .unwrap_or_default()
                    };
                    if !namespace_names.is_empty()
                        && let Some(other_file) = self.files.get(other_name)
                    {
                        let mut locations = Vec::new();
                        for namespace_name in namespace_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &namespace_name,
                                &export_name,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            let mut seen_namespace_targets: FxHashSet<(String, String, String)> =
                FxHashSet::default();
            for target in namespace_targets {
                if !seen_namespace_targets.insert((
                    target.file.clone(),
                    target.namespace.clone(),
                    target.member.clone(),
                )) {
                    continue;
                }

                for other_name in &file_names {
                    if other_name == &target.file {
                        continue;
                    }

                    let local_names = {
                        let other_file = self.files.get(other_name);
                        other_file
                            .map(|file| {
                                self.named_import_local_names(file, &target.file, &target.namespace)
                            })
                            .unwrap_or_default()
                    };
                    if local_names.is_empty() {
                        continue;
                    }

                    if let Some(other_file) = self.files.get(other_name) {
                        let mut locations = Vec::new();
                        for local_name in local_names {
                            self.collect_namespace_member_locations(
                                other_file,
                                &local_name,
                                &target.member,
                                &mut locations,
                            );
                        }
                        for location in locations {
                            workspace_edit.add_edit(
                                location.file_path,
                                TextEdit::new(location.range, normalized_name.clone()),
                            );
                        }
                    }
                }
            }

            Self::dedup_workspace_edit(&mut workspace_edit);
            Ok(workspace_edit)
        })();

        self.performance
            .record(ProjectRequestKind::Rename, start.elapsed(), scope_stats);

        result
    }

    fn definition_from_import(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<Vec<Location>> {
        let target = self.import_target_at_position(file, position)?;
        let resolved = self.resolve_module_specifier(file.file_name(), &target.module_specifier)?;
        let target_file = self.files.get(&resolved)?;

        match target.kind {
            ImportKind::Namespace => {
                let location = target_file.node_location(target_file.root())?;
                Some(vec![location])
            }
            ImportKind::Default => {
                let locations = target_file.export_locations("default");
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
            ImportKind::Named(name) => {
                let locations = target_file.export_locations(&name);
                if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                }
            }
        }
    }

    fn import_candidates_for_diagnostics(
        &self,
        file: &ProjectFile,
        diagnostics: &[LspDiagnostic],
    ) -> Vec<ImportCandidate> {
        let mut candidates = Vec::new();
        let mut seen = FxHashSet::default();

        for diag in diagnostics {
            if diag.code
                != Some(crate::checker::types::diagnostics::diagnostic_codes::CANNOT_FIND_NAME)
            {
                continue;
            }

            let Some(missing_name) = self.identifier_at_range(file, diag.range) else {
                continue;
            };

            self.collect_import_candidates_for_name(
                file,
                &missing_name,
                &mut candidates,
                &mut seen,
            );
        }

        candidates
    }

    fn collect_import_candidates_for_name(
        &self,
        from_file: &ProjectFile,
        missing_name: &str,
        output: &mut Vec<ImportCandidate>,
        seen: &mut FxHashSet<(String, String, String, bool)>,
    ) {
        for file_name in self.files.keys() {
            if file_name == from_file.file_name() {
                continue;
            }

            let Some(module_specifier) =
                self.module_specifier_from_files(from_file.file_name(), file_name)
            else {
                continue;
            };

            let mut visited = FxHashSet::default();
            let matches = self.matching_exports_in_file(file_name, missing_name, &mut visited);

            for export_match in matches {
                let candidate = ImportCandidate {
                    module_specifier: module_specifier.clone(),
                    local_name: missing_name.to_string(),
                    kind: export_match.kind,
                    is_type_only: export_match.is_type_only,
                };

                let kind_key = match &candidate.kind {
                    ImportCandidateKind::Named { export_name } => format!("named:{}", export_name),
                    ImportCandidateKind::Default => "default".to_string(),
                    ImportCandidateKind::Namespace => "namespace".to_string(),
                };

                if seen.insert((
                    candidate.module_specifier.clone(),
                    candidate.local_name.clone(),
                    kind_key,
                    candidate.is_type_only,
                )) {
                    output.push(candidate);
                }
            }
        }
    }

    fn completion_from_import_candidate(&self, candidate: &ImportCandidate) -> CompletionItem {
        let detail = self.auto_import_detail(candidate);
        let documentation = self.auto_import_documentation(candidate);

        let mut item =
            CompletionItem::new(candidate.local_name.clone(), CompletionItemKind::Variable);
        item = item.with_detail(detail);
        if let Some(doc) = documentation {
            item = item.with_documentation(doc);
        }
        item
    }

    fn auto_import_detail(&self, candidate: &ImportCandidate) -> String {
        let prefix = if candidate.is_type_only {
            "auto-import type"
        } else {
            "auto-import"
        };

        match candidate.kind {
            ImportCandidateKind::Named { .. } => {
                format!("{} from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Default => {
                format!("{} default from {}", prefix, candidate.module_specifier)
            }
            ImportCandidateKind::Namespace => {
                format!("{} namespace from {}", prefix, candidate.module_specifier)
            }
        }
    }

    fn auto_import_documentation(&self, candidate: &ImportCandidate) -> Option<String> {
        let import_kw = if candidate.is_type_only {
            "import type"
        } else {
            "import"
        };

        let snippet = match &candidate.kind {
            ImportCandidateKind::Named { export_name } => {
                format!(
                    "{} {{ {} }} from \"{}\";",
                    import_kw, export_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Default => {
                format!(
                    "{} {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
            ImportCandidateKind::Namespace => {
                format!(
                    "{} * as {} from \"{}\";",
                    import_kw, candidate.local_name, candidate.module_specifier
                )
            }
        };

        Some(snippet)
    }

    fn matching_exports_in_file(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> Vec<ExportMatch> {
        if !visited.insert(file_name.to_string()) {
            return Vec::new();
        }

        let Some(file) = self.files.get(file_name) else {
            return Vec::new();
        };
        let arena = file.arena();
        let Some(root_node) = arena.get(file.root()) else {
            return Vec::new();
        };
        let Some(source_file) = arena.get_source_file(root_node) else {
            return Vec::new();
        };

        let mut matches = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }

            let Some(export) = arena.get_export_decl(stmt_node) else {
                continue;
            };

            if export.is_default_export {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Default,
                    is_type_only: export.is_type_only,
                });
                continue;
            }

            if export.module_specifier.is_none() {
                if export.export_clause.is_none() {
                    continue;
                }

                let Some(clause_node) = arena.get(export.export_clause) else {
                    continue;
                };
                if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                    let Some(named) = arena.get_named_imports(clause_node) else {
                        continue;
                    };
                    for &spec_idx in &named.elements.nodes {
                        let Some(spec_node) = arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(spec) = arena.get_specifier(spec_node) else {
                            continue;
                        };

                        let export_ident = if !spec.name.is_none() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        let Some(export_text) = arena.get_identifier_text(export_ident) else {
                            continue;
                        };
                        if export_text == "default" {
                            matches.push(ExportMatch {
                                kind: ImportCandidateKind::Default,
                                is_type_only: export.is_type_only || spec.is_type_only,
                            });
                        }
                        if export_text != export_name {
                            continue;
                        }

                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Named {
                                export_name: export_text.to_string(),
                            },
                            is_type_only: export.is_type_only || spec.is_type_only,
                        });
                    }
                } else if file.declaration_has_name(export.export_clause, export_name) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let module_specifier = match arena.get_literal_text(export.module_specifier) {
                Some(text) => text,
                None => continue,
            };
            let resolved = match self.resolve_module_specifier(file.file_name(), module_specifier) {
                Some(path) => path,
                None => continue,
            };

            if export.export_clause.is_none() {
                if export_name == "default" {
                    continue;
                }

                if self.file_exports_named(&resolved, export_name, visited) {
                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_name.to_string(),
                        },
                        is_type_only: export.is_type_only,
                    });
                }

                continue;
            }

            let Some(clause_node) = arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                let Some(named) = arena.get_named_imports(clause_node) else {
                    continue;
                };
                for &spec_idx in &named.elements.nodes {
                    let Some(spec_node) = arena.get(spec_idx) else {
                        continue;
                    };
                    let Some(spec) = arena.get_specifier(spec_node) else {
                        continue;
                    };

                    let export_ident = if !spec.name.is_none() {
                        spec.name
                    } else {
                        spec.property_name
                    };
                    let Some(export_text) = arena.get_identifier_text(export_ident) else {
                        continue;
                    };
                    if export_text == "default" {
                        matches.push(ExportMatch {
                            kind: ImportCandidateKind::Default,
                            is_type_only: export.is_type_only || spec.is_type_only,
                        });
                    }
                    if export_text != export_name {
                        continue;
                    }

                    matches.push(ExportMatch {
                        kind: ImportCandidateKind::Named {
                            export_name: export_text.to_string(),
                        },
                        is_type_only: export.is_type_only || spec.is_type_only,
                    });
                }
            } else if clause_node.kind == SyntaxKind::Identifier as u16
                && let Some(export_text) = arena.get_identifier_text(export.export_clause)
                && export_text == export_name
            {
                matches.push(ExportMatch {
                    kind: ImportCandidateKind::Named {
                        export_name: export_text.to_string(),
                    },
                    is_type_only: export.is_type_only,
                });
            }
        }

        matches
    }

    fn file_exports_named(
        &self,
        file_name: &str,
        export_name: &str,
        visited: &mut FxHashSet<String>,
    ) -> bool {
        self.matching_exports_in_file(file_name, export_name, visited)
            .iter()
            .any(|export_match| matches!(export_match.kind, ImportCandidateKind::Named { .. }))
    }

    fn identifier_at_range(&self, file: &ProjectFile, range: Range) -> Option<String> {
        let offset = file
            .line_map()
            .position_to_offset(range.start, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        file.arena()
            .get_identifier_text(node_idx)
            .map(|text| text.to_string())
    }

    fn identifier_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<(NodeIndex, String)> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let mut node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() && offset > 0 {
            node_idx = find_node_at_offset(file.arena(), offset - 1);
        }
        if node_idx.is_none() {
            return None;
        }

        let node = file.arena().get(node_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let text = file.arena().get_identifier_text(node_idx)?.to_string();
        Some((node_idx, text))
    }

    fn is_member_access_node(&self, arena: &NodeArena, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while !current.is_none() {
            let Some(node) = arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || node.kind == syntax_kind_ext::QUALIFIED_NAME
            {
                return true;
            }

            let Some(ext) = arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        false
    }

    fn import_target_at_position(
        &self,
        file: &ProjectFile,
        position: Position,
    ) -> Option<ImportTarget> {
        let offset = file
            .line_map()
            .position_to_offset(position, file.source_text())?;
        let node_idx = find_node_at_offset(file.arena(), offset);
        if node_idx.is_none() {
            return None;
        }
        self.import_target_from_node(file, node_idx)
    }

    fn import_target_from_node(
        &self,
        file: &ProjectFile,
        node_idx: NodeIndex,
    ) -> Option<ImportTarget> {
        let arena = file.arena();
        let mut current = node_idx;
        let mut import_specifier = None;
        let mut import_clause = None;
        let mut import_decl = None;

        while !current.is_none() {
            let node = arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::IMPORT_SPECIFIER => {
                    import_specifier = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_CLAUSE => {
                    import_clause = Some(current);
                }
                k if k == syntax_kind_ext::IMPORT_DECLARATION
                    || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                {
                    import_decl = Some(current);
                    break;
                }
                _ => {}
            }
            current = arena.get_extended(current)?.parent;
        }

        let import_decl_idx = import_decl?;
        let import_decl_node = arena.get(import_decl_idx)?;
        let import_decl = arena.get_import_decl(import_decl_node)?;
        let module_specifier = arena
            .get_literal_text(import_decl.module_specifier)?
            .to_string();

        let kind = if let Some(spec_idx) = import_specifier {
            let spec_node = arena.get(spec_idx)?;
            let spec = arena.get_specifier(spec_node)?;
            let export_ident = if !spec.property_name.is_none() {
                spec.property_name
            } else {
                spec.name
            };
            let export_name = arena.get_identifier_text(export_ident)?.to_string();
            ImportKind::Named(export_name)
        } else if let Some(clause_idx) = import_clause {
            let clause_node = arena.get(clause_idx)?;
            let clause = arena.get_import_clause(clause_node)?;

            if clause.name == node_idx {
                ImportKind::Default
            } else if clause.named_bindings == node_idx {
                ImportKind::Namespace
            } else if import_decl.module_specifier == node_idx {
                ImportKind::Namespace
            } else {
                return None;
            }
        } else if import_decl.module_specifier == node_idx {
            ImportKind::Namespace
        } else {
            return None;
        };

        Some(ImportTarget {
            module_specifier,
            kind,
        })
    }

    fn resolve_module_specifier(&self, from_file: &str, module_specifier: &str) -> Option<String> {
        let candidates = self.module_specifier_candidates(from_file, module_specifier);
        candidates
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
    }

    fn module_specifier_from_files(&self, from_file: &str, target_file: &str) -> Option<String> {
        let from_dir = Path::new(from_file)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let target_path = strip_ts_extension(Path::new(target_file));
        let relative = relative_path(from_dir, &target_path);

        let mut spec = path_to_string(&relative).replace('\\', "/");
        if spec.is_empty() {
            return None;
        }
        if !spec.starts_with('.') {
            spec = format!("./{}", spec);
        }
        Some(spec)
    }

    fn module_specifier_candidates(&self, from_file: &str, module_specifier: &str) -> Vec<String> {
        let mut candidates = Vec::new();

        if module_specifier.starts_with('.') {
            let base_dir = Path::new(from_file)
                .parent()
                .unwrap_or_else(|| Path::new(""));
            let joined = normalize_path(&base_dir.join(module_specifier));

            if joined.extension().is_some() {
                candidates.push(path_to_string(&joined));
            } else {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.with_extension(ext)));
                }
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(path_to_string(&joined.join("index").with_extension(ext)));
                }
            }
        } else {
            candidates.push(module_specifier.to_string());
            if Path::new(module_specifier).extension().is_none() {
                for ext in TS_EXTENSION_CANDIDATES {
                    candidates.push(format!("{}.{}", module_specifier, ext));
                }
            }
        }

        candidates
    }
}

const TS_EXTENSION_CANDIDATES: [&str; 7] = ["ts", "tsx", "d.ts", "mts", "cts", "d.mts", "d.cts"];
const TS_EXTENSION_SUFFIXES: [&str; 7] =
    [".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts"];

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Normal(_) | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn strip_ts_extension(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };

    for suffix in TS_EXTENSION_SUFFIXES {
        if let Some(base_name) = file_name.strip_suffix(suffix) {
            if base_name.is_empty() {
                return path.to_path_buf();
            }
            let mut base = PathBuf::new();
            if let Some(parent) = path.parent() {
                base.push(parent);
            }
            base.push(base_name);
            return base;
        }
    }

    path.to_path_buf()
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from_components: Vec<_> = from
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();
    let to_components: Vec<_> = to
        .components()
        .filter(|c| *c != Component::CurDir)
        .collect();

    let mut common = 0;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
