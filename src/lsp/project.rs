//! Project container for multi-file LSP operations.
//!
//! This provides a lightweight home for parsed files, binders, and line maps so
//! LSP features can be extended across multiple files.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::time::{Duration, Instant};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::binder::BinderState;
use crate::binder::SymbolId;
use crate::checker::TypeCache;
use crate::checker::state::CheckerState;
#[cfg(not(target_arch = "wasm32"))]
use crate::cli::config::{load_tsconfig, resolve_compiler_options};
use crate::lsp::code_actions::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionProvider, ImportCandidateKind,
};
use crate::lsp::completions::{CompletionItem, Completions};
use crate::lsp::definition::GoToDefinition;
use crate::lsp::dependency_graph::DependencyGraph;
use crate::lsp::diagnostics::{LspDiagnostic, convert_diagnostic};
use crate::lsp::hover::{HoverInfo, HoverProvider};
use crate::lsp::position::{LineMap, Location, Position, Range};
use crate::lsp::rename::{TextEdit, WorkspaceEdit};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats};
use crate::lsp::signature_help::{SignatureHelp, SignatureHelpProvider};
use crate::parser::ParserState;
use crate::parser::node::NodeAccess;
use crate::parser::{NodeIndex, NodeList, node::NodeArena, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::solver::TypeInterner;

pub(crate) enum ImportKind {
    Named(String),
    Default,
    Namespace,
}

pub(crate) struct ImportTarget {
    pub(crate) module_specifier: String,
    pub(crate) kind: ImportKind,
}

/// A file rename request from the LSP client.
pub struct FileRename {
    /// The original file path (URI)
    pub old_uri: String,
    /// The new file path (URI)
    pub new_uri: String,
}

pub(crate) struct NamespaceReexportTarget {
    pub(crate) file: String,
    pub(crate) namespace: String,
    pub(crate) member: String,
}

pub(crate) struct ExportMatch {
    pub(crate) kind: ImportCandidateKind,
    pub(crate) is_type_only: bool,
}

pub(crate) struct ImportSpecifierTarget {
    pub(crate) local_ident: NodeIndex,
    pub(crate) property_name: Option<NodeIndex>,
}

struct IncrementalUpdatePlan {
    reparse_start: u32,
    prefix_nodes: Vec<NodeIndex>,
}

const INCREMENTAL_NODE_MULTIPLIER: usize = 4;
const INCREMENTAL_MIN_NODE_BUDGET: usize = 4096;

/// Parsed file state used by LSP features.
pub struct ProjectFile {
    pub(crate) file_name: String,
    pub(crate) root: NodeIndex,
    pub(crate) parser: ParserState,
    pub(crate) binder: BinderState,
    pub(crate) line_map: LineMap,
    pub(crate) type_interner: TypeInterner,
    pub(crate) type_cache: Option<TypeCache>,
    pub(crate) scope_cache: ScopeCache,
    pub(crate) strict: bool,
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

        let query_cache = crate::solver::QueryCache::new(&self.type_interner);

        let mut checker = if let Some(cache) = self.type_cache.take() {
            CheckerState::with_cache(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
                file_name,
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
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

    pub(crate) fn node_location(&self, node_idx: NodeIndex) -> Option<Location> {
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

    pub(crate) fn export_locations(&self, export_name: &str) -> Vec<Location> {
        self.export_nodes(export_name)
            .into_iter()
            .filter_map(|node| self.node_location(node))
            .collect()
    }

    pub(crate) fn export_nodes(&self, export_name: &str) -> Vec<NodeIndex> {
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

    pub(crate) fn exported_names_for_symbol(&self, sym_id: SymbolId) -> Vec<String> {
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

    pub(crate) fn import_targets_for_local(&self, local_name: &str) -> Vec<ImportTarget> {
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

    pub(crate) fn declaration_has_name(&self, decl_idx: NodeIndex, export_name: &str) -> bool {
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
    pub(crate) fn record(
        &mut self,
        kind: ProjectRequestKind,
        duration: Duration,
        stats: ScopeCacheStats,
    ) {
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
    pub(crate) files: FxHashMap<String, ProjectFile>,
    pub(crate) dependency_graph: DependencyGraph,
    pub(crate) performance: ProjectPerformance,
    pub(crate) strict: bool,
}

impl Project {
    /// Create a new empty project.
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
            dependency_graph: DependencyGraph::new(),
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
        self.files.insert(file_name.clone(), file);
        // Update dependency graph with imports from this file
        self.update_dependencies(&file_name);
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

    /// Extract import paths from a file's AST.
    ///
    /// Walks the top-level statements to find ImportDeclaration,
    /// ExportDeclaration, and dynamic imports (import(), require()) nodes
    /// and extracts their module specifier strings for the DependencyGraph.
    fn extract_imports(&self, file_name: &str) -> Vec<String> {
        let mut imports = Vec::new();

        let file = match self.files.get(file_name) {
            Some(f) => f,
            None => return imports,
        };

        let arena = file.arena();
        let _root = file.root();
        let source_text = file.source_text();

        // Walk all nodes to find ImportDeclaration, ExportDeclaration, and CallExpression
        // In the flat NodeArena, we iterate over all nodes
        for (i, node) in arena.nodes.iter().enumerate() {
            let node_idx = NodeIndex(i as u32);

            // Get the module specifier node for imports, exports, and dynamic imports
            let specifier_idx = if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                arena.get_import_decl(node).map(|d| d.module_specifier)
            } else if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                // Handle: export ... from "module" (re-exports)
                arena.get_export_decl(node).map(|d| d.module_specifier)
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Check for dynamic imports: import("./module") or require("./module")
                self.try_extract_dynamic_import(node_idx, arena)
            } else {
                None
            };

            if let Some(specifier_idx) = specifier_idx {
                if specifier_idx.is_none() {
                    continue;
                }

                let specifier_node = match arena.get(specifier_idx) {
                    Some(n) => n,
                    None => continue,
                };

                // Extract the string text
                let start = specifier_node.pos as usize;
                let end = specifier_node.end as usize;

                if end <= start || end > source_text.len() {
                    continue;
                }

                let text = &source_text[start..end];

                // Remove quotes to get just the path
                let path = if text.starts_with('"') && text.ends_with('"') && text.len() > 1 {
                    &text[1..text.len() - 1]
                } else if text.starts_with('\'') && text.ends_with('\'') && text.len() > 1 {
                    &text[1..text.len() - 1]
                } else {
                    continue;
                };

                imports.push(path.to_string());
            }
        }

        imports
    }

    /// Try to extract a dynamic import specifier from a call expression.
    /// Returns the node index of the specifier string if this is import() or require().
    fn try_extract_dynamic_import(
        &self,
        call_idx: NodeIndex,
        arena: &NodeArena,
    ) -> Option<NodeIndex> {
        let call_node = arena.get(call_idx)?;
        let call_data = arena.get_call_expr(call_node)?;

        // Check if this is import() or require()
        let is_import = self.is_import_keyword(call_data.expression, arena);
        let is_require = self.is_require_identifier(call_data.expression, arena);

        if !is_import && !is_require {
            return None;
        }

        // Get the first argument (the module specifier)
        let args = call_data.arguments.as_ref()?;
        args.nodes.first().copied()
    }

    /// Check if a node is the `import` keyword (for dynamic import expressions).
    fn is_import_keyword(&self, node_idx: NodeIndex, arena: &NodeArena) -> bool {
        use crate::scanner::SyntaxKind;

        if node_idx.is_none() {
            return false;
        }
        if let Some(node) = arena.get(node_idx) {
            node.kind == SyntaxKind::ImportKeyword as u16
        } else {
            false
        }
    }

    /// Check if a node is a `require` identifier.
    fn is_require_identifier(&self, node_idx: NodeIndex, arena: &NodeArena) -> bool {
        use crate::scanner::SyntaxKind;

        if node_idx.is_none() {
            return false;
        }
        if let Some(node) = arena.get(node_idx) {
            if node.kind != SyntaxKind::Identifier as u16 {
                return false;
            }
            if let Some(ident_data) = arena.get_identifier(node) {
                ident_data.escaped_text == "require"
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Update the dependency graph for a file.
    ///
    /// Extracts imports from the file's AST and updates the DependencyGraph.
    /// This should be called whenever a file is added or modified.
    fn update_dependencies(&mut self, file_name: &str) {
        let imports = self.extract_imports(file_name);
        self.dependency_graph.update_file(file_name, &imports);
    }

    /// Handle file rename requests from the LSP client.
    ///
    /// When files are renamed or moved, this calculates the TextEdits needed
    /// to update import statements in all dependent files.
    ///
    /// # Arguments
    /// * `renames` - List of file renames (old path -> new path)
    ///
    /// # Returns
    /// A WorkspaceEdit containing all the TextEdits needed to update imports
    ///
    /// # Example
    /// ```ignore
    /// // When utils.ts moves to src/utils.ts
    /// let renames = vec![FileRename {
    ///     old_uri: "/project/utils.ts".to_string(),
    ///     new_uri: "/project/src/utils.ts".to_string(),
    /// }];
    /// let edits = project.handle_will_rename_files(&renames);
    /// // Returns edits for all files that import utils.ts
    /// ```
    pub fn handle_will_rename_files(&mut self, renames: &[FileRename]) -> WorkspaceEdit {
        use std::path::Path;

        let mut result = WorkspaceEdit::new();

        for rename in renames {
            let old_path = Path::new(&rename.old_uri);
            let new_path = Path::new(&rename.new_uri);

            // Check if this is a directory rename
            if self.is_directory(old_path) {
                // Directory rename: expand to individual file renames
                let files_in_dir = self.find_files_in_directory(old_path);

                for old_file_path in files_in_dir {
                    // Compute the new path for this file
                    // Relative path within the directory
                    let relative = old_file_path
                        .strip_prefix(&rename.old_uri)
                        .unwrap_or(&old_file_path);
                    let new_file_path = new_path.join(relative);
                    let new_file_path_str = new_file_path.to_string_lossy().to_string();

                    // Process this file rename with the actual file paths (not directory)
                    self.process_file_rename(
                        Path::new(&old_file_path),
                        Path::new(&new_file_path_str),
                        &mut result,
                    );
                }
            } else {
                // Single file rename
                self.process_file_rename(old_path, new_path, &mut result);
            }
        }

        result
    }

    /// Process a single file rename (internal helper).
    ///
    /// Updates imports in all dependent files that reference the renamed file.
    fn process_file_rename(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        result: &mut WorkspaceEdit,
    ) {
        use crate::lsp::file_rename::FileRenameProvider;
        use crate::lsp::rename::TextEdit;
        use crate::lsp::utils::calculate_new_relative_path;
        use std::path::Path;

        // Iterate through all files to find those that import the renamed file
        // We can't use dependency_graph.get_dependents() directly because it stores
        // raw import specifiers (e.g., "./utils/math") not resolved file paths
        for (dependent_path, dep_file) in self.files.iter() {
            // Create a provider to find import nodes
            let provider = FileRenameProvider::new(
                dep_file.arena(),
                dep_file.line_map(),
                dep_file.source_text(),
            );

            // Find all import/export specifiers in this file
            let import_locations = provider.find_import_specifier_nodes(dep_file.root());

            // For each import, check if it needs updating
            for import_loc in import_locations {
                // CRITICAL: Check if this import actually points to the renamed file
                // Without this check, we would rewrite ALL imports in the file
                let dependent_path_obj = Path::new(dependent_path);
                if !self.is_import_pointing_to_file(
                    dependent_path_obj,
                    &import_loc.current_specifier,
                    old_path,
                ) {
                    // This import doesn't point to the renamed file, skip it
                    continue;
                }

                // Calculate the new import path
                if let Some(new_specifier) = calculate_new_relative_path(
                    Path::new(dependent_path),
                    old_path,
                    new_path,
                    &import_loc.current_specifier,
                ) {
                    // Create a TextEdit for this import
                    let text_edit = TextEdit::new(import_loc.range, new_specifier);

                    // Add to the result
                    result.add_edit(dependent_path.clone(), text_edit);
                }
            }
        }

        // Update the dependency graph to reflect the rename
        // Note: The dependency graph uses raw import specifiers, not resolved paths
        // So we can't directly update it here. The graph will be rebuilt when
        // files are re-parsed/re-checked in the normal workflow.
    }

    /// Fetch a file by name.
    pub fn file(&self, file_name: &str) -> Option<&ProjectFile> {
        self.files.get(file_name)
    }

    /// Check if an import specifier points to a specific target file path.
    ///
    /// This is a simplified check that handles basic relative path resolution.
    /// It verifies if the specifier, when joined with the importer's directory,
    /// resolves to the target file path.
    ///
    /// # Arguments
    /// * `importer` - Path of the file containing the import
    /// * `specifier` - The import specifier (e.g., "./utils" or "../types")
    /// * `target` - The target file path we're checking against
    fn is_import_pointing_to_file(&self, importer: &Path, specifier: &str, target: &Path) -> bool {
        let importer_dir = match importer.parent() {
            Some(p) => p,
            None => return false,
        };

        // Simple resolution: join dir + specifier
        let resolved = importer_dir.join(specifier);

        // Normalize the path by resolving .. and . components
        let normalized = self.normalize_path(&resolved);

        // Check exact match
        let target_str = target.to_string_lossy();
        if normalized == target_str {
            return true;
        }

        // Check with extensions (TypeScript resolution logic simplified)
        // The specifier might not have an extension, so we check stems
        let normalized_path = Path::new(&normalized);
        if let Some(target_stem) = target.file_stem() {
            if let Some(resolved_stem) = normalized_path.file_stem() {
                if target_stem == resolved_stem {
                    // Normalize target as well for comparison
                    let normalized_target = self.normalize_path(target);
                    let normalized_target_path = Path::new(&normalized_target);
                    // Check if parent dirs match
                    if normalized_path.parent() == normalized_target_path.parent() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Simple path normalization that resolves . and .. components without filesystem access.
    fn normalize_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Split by / and process components
        let components: Vec<&str> = path_str.split('/').collect();
        let mut result = Vec::new();

        for component in components {
            if component == "." {
                // Skip current directory component
                continue;
            } else if component == ".." {
                // Pop from result if possible
                if !result.is_empty() && result.last() != Some(&"") {
                    result.pop();
                }
            } else {
                result.push(component);
            }
        }

        result.join("/")
    }

    /// Check if a path represents a directory (vs a file).
    ///
    /// This is a heuristic check for LSP file rename operations.
    /// In a real LSP server, you would use file system metadata, but here
    /// we check if the path exists in our project as a prefix to other files.
    fn is_directory(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let path_str_ref = path_str.as_ref();

        // Check if any file in the project has this path as a prefix
        for file_path in self.files.keys() {
            if file_path.starts_with(path_str_ref) {
                // Ensure it's a proper directory separator
                let rest = &file_path[path_str_ref.len()..];
                if rest.starts_with('/') || rest.starts_with('\\') {
                    return true;
                }
            }
        }

        false
    }

    /// Recursively find all TypeScript files within a directory path.
    ///
    /// Returns all .ts and .tsx files that have the given directory as a prefix.
    fn find_files_in_directory(&self, directory: &Path) -> Vec<String> {
        let dir_str = directory.to_string_lossy();
        let dir_str_ref = dir_str.as_ref();
        let mut result = Vec::new();

        for file_path in self.files.keys() {
            if file_path.starts_with(dir_str_ref) {
                // Check if it's a .ts or .tsx file (not a directory)
                if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
                    result.push(file_path.clone());
                }
            }
        }

        result
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

            // Create CodeActionProvider for generating import edits
            use crate::lsp::code_actions::CodeActionProvider;
            let code_action_provider = CodeActionProvider::new(
                file.arena(),
                &file.binder,
                &file.line_map,
                file.file_name.clone(),
                file.source_text(),
            );

            for candidate in candidates {
                if existing.contains(&candidate.local_name) {
                    continue;
                }

                let mut item = self.completion_from_import_candidate(&candidate);

                // Generate additional text edits for auto-import
                if let Some(edits) =
                    code_action_provider.build_auto_import_edit(file.root(), &candidate)
                {
                    item = item.with_additional_edits(edits);
                }

                completions.push(item);
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
}
