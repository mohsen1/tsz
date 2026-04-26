use std::hash::{Hash, Hasher};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::Arc;
use web_time::{Duration, Instant};

use globset::Glob;
use regex::{Regex, RegexBuilder};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

use crate::code_actions::ImportCandidateKind;
use crate::completions::{CompletionItem, Completions};
use crate::dependency_graph::DependencyGraph;
use crate::diagnostics::{LspDiagnostic, convert_diagnostic};
use crate::export_signature::{ExportSignature, InvalidationSummary};
use crate::hover::{HoverInfo, HoverProvider};
use crate::rename::TextEdit;
#[cfg(not(target_arch = "wasm32"))]
use crate::rename::WorkspaceEdit;
use crate::resolver::{ScopeCache, ScopeCacheStats};
use crate::signature_help::{SignatureHelp, SignatureHelpProvider};
use crate::symbols::symbol_index::SymbolIndex;
use tsz_binder::BinderState;
use tsz_binder::SymbolId;
use tsz_checker::TypeCache;
use tsz_checker::state::CheckerState;
use tsz_common::position::{LineMap, Location, Position, Range};
use tsz_parser::ParserState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;
use tsz_solver::def::DefinitionStore;

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
    /// Shared type interner for cross-file type identity.
    ///
    /// All files in a `Project` share the same `TypeInterner` via `Arc`,
    /// ensuring that `TypeId`s are globally unique and cross-file type
    /// comparisons use identity checks rather than structural matching.
    /// When used standalone (outside a `Project`), a per-file interner is created.
    pub(crate) type_interner: Arc<TypeInterner>,
    /// Shared definition store for cross-file `DefId` consistency.
    ///
    /// When present, per-file checkers use `with_cache_and_shared_def_store` /
    /// `new_with_shared_def_store` so that all `DefId`s resolve through a single
    /// global `DefinitionStore` owned by the `Project`.
    pub(crate) definition_store: Option<Arc<DefinitionStore>>,
    pub(crate) type_cache: Option<TypeCache>,
    pub(crate) scope_cache: ScopeCache,
    pub(crate) strict: bool,
    /// Flag indicating if caches were invalidated and diagnostics need re-computation
    pub(crate) diagnostics_dirty: bool,
    /// Position-independent hash of the file's public API (exports, re-exports, augmentations).
    /// Used to avoid invalidating dependent files when only function bodies or comments change.
    pub(crate) export_signature: ExportSignature,
    /// Content hash of the source text.
    ///
    /// Used to skip redundant re-parse and re-bind when `set_file` is called with
    /// identical content (e.g., `didOpen` on an already-loaded file, or `didSave`
    /// without changes). Computed via `FxHasher` for speed.
    pub(crate) content_hash: u64,
    /// Stable file index assigned by the `Project`'s `FileIdAllocator`.
    ///
    /// Used as the `file_id` in `DefinitionStore` registrations, enabling
    /// per-file invalidation when a file is removed or replaced. The binder's
    /// symbols have their `decl_file_idx` set to this value, so all
    /// `DefinitionInfo` records created by the checker carry the correct
    /// file provenance.
    ///
    /// `u32::MAX` means no stable ID was assigned (standalone mode).
    pub(crate) file_idx: u32,
    /// Timestamp of the last LSP operation that accessed this file.
    ///
    /// Updated by `touch()` when the file is used for diagnostics, hover,
    /// completions, definitions, or references. Used by eviction heuristics
    /// to identify cold files that can be dropped under memory pressure.
    pub(crate) last_accessed: Instant,
}

/// Compute a fast content hash for source text.
///
/// Uses `FxHasher` for speed — this is not cryptographic, just a change-detection
/// fingerprint. Collisions are extremely unlikely for source text of different content.
fn hash_source_content(source: &str) -> u64 {
    let mut hasher = FxHasher::default();
    source.hash(&mut hasher);
    hasher.finish()
}

impl ProjectFile {
    /// Parse and bind a single source file for LSP queries.
    ///
    /// Creates a standalone file with its own `TypeInterner`. For files
    /// within a `Project`, use `with_shared_interner` instead.
    pub fn new(file_name: String, source_text: String) -> Self {
        Self::with_strict(file_name, source_text, false)
    }

    /// Parse and bind a single source file with explicit strict mode setting.
    ///
    /// Creates a standalone file with its own `TypeInterner`. For files
    /// within a `Project`, use `with_shared_interner` instead.
    pub fn with_strict(file_name: String, source_text: String, strict: bool) -> Self {
        Self::with_shared_interner(
            file_name,
            source_text,
            strict,
            Arc::new(TypeInterner::new()),
        )
    }

    /// Parse and bind a single source file with a shared `TypeInterner`.
    ///
    /// All files sharing the same interner will have globally unique `TypeId`s,
    /// enabling O(1) cross-file type identity checks.
    pub fn with_shared_interner(
        file_name: String,
        source_text: String,
        strict: bool,
        type_interner: Arc<TypeInterner>,
    ) -> Self {
        Self::with_shared_interner_and_file_idx(
            file_name,
            source_text,
            strict,
            type_interner,
            u32::MAX,
        )
    }

    /// Parse and bind a single source file with a shared `TypeInterner` and
    /// a driver-assigned stable file index.
    ///
    /// The `file_idx` is stamped onto all binder symbols (`decl_file_idx`)
    /// during binding, enabling per-file `DefinitionStore` invalidation.
    /// Pass `u32::MAX` for standalone mode (no invalidation tracking).
    fn with_shared_interner_and_file_idx(
        file_name: String,
        source_text: String,
        strict: bool,
        type_interner: Arc<TypeInterner>,
        file_idx: u32,
    ) -> Self {
        let content_hash = hash_source_content(&source_text);
        let mut parser = ParserState::new(file_name.clone(), source_text);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        if file_idx != u32::MAX {
            binder.set_file_idx(file_idx);
        }
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(parser.get_source_text());
        let export_signature = ExportSignature::compute(&binder, &file_name);

        Self {
            file_name,
            root,
            parser,
            binder,
            line_map,
            type_interner,
            definition_store: None,
            type_cache: None,
            scope_cache: ScopeCache::default(),
            strict,
            diagnostics_dirty: false,
            export_signature,
            content_hash,
            file_idx,
            last_accessed: Instant::now(),
        }
    }

    /// Parse and bind a single source file with both a shared `TypeInterner`
    /// and a shared `DefinitionStore`.
    ///
    /// This is the preferred constructor for files within a `Project`, ensuring
    /// that both `TypeId`s and `DefId`s are globally unique across files.
    pub fn with_shared_interner_and_def_store(
        file_name: String,
        source_text: String,
        strict: bool,
        type_interner: Arc<TypeInterner>,
        definition_store: Arc<DefinitionStore>,
    ) -> Self {
        let mut file = Self::with_shared_interner(file_name, source_text, strict, type_interner);
        file.definition_store = Some(definition_store);
        file
    }

    /// Parse and bind a file with shared interner, shared def store, and a
    /// driver-assigned stable file index for per-file invalidation.
    ///
    /// This is the full constructor used by `Project::set_file`. The `file_idx`
    /// is stamped onto binder symbols so that `DefinitionStore::invalidate_file`
    /// can later clean up all definitions from this file.
    fn with_full_project_context(
        file_name: String,
        source_text: String,
        strict: bool,
        type_interner: Arc<TypeInterner>,
        definition_store: Arc<DefinitionStore>,
        file_idx: u32,
    ) -> Self {
        let mut file = Self::with_shared_interner_and_file_idx(
            file_name,
            source_text,
            strict,
            type_interner,
            file_idx,
        );
        file.definition_store = Some(definition_store);
        file
    }

    /// File name used for LSP locations.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Root node of the parsed source file.
    pub const fn root(&self) -> NodeIndex {
        self.root
    }

    /// Arena containing parsed Nodes.
    pub const fn arena(&self) -> &NodeArena {
        self.parser.get_arena()
    }

    /// Binder state for symbol lookup.
    pub const fn binder(&self) -> &BinderState {
        &self.binder
    }

    /// Line map for offset <-> position conversions.
    pub const fn line_map(&self) -> &LineMap {
        &self.line_map
    }

    /// Original source text for this file.
    pub fn source_text(&self) -> &str {
        self.parser.get_source_text()
    }

    /// Borrowed view of the inputs binder-tier LSP providers consume.
    ///
    /// Combines [`Self::arena`], [`Self::binder`], [`Self::line_map`],
    /// [`Self::file_name`], and [`Self::source_text`] into a single
    /// [`super::LspProviderContext`] so feature dispatch can build providers
    /// via `Provider::from_context(file.provider_context())` instead of
    /// repeating the five accessors at every call site.
    pub fn provider_context(&self) -> super::LspProviderContext<'_> {
        super::LspProviderContext {
            arena: self.arena(),
            binder: self.binder(),
            line_map: self.line_map(),
            file_name: self.file_name(),
            source_text: self.source_text(),
        }
    }

    /// Content hash of the source text.
    ///
    /// This is a fast (non-cryptographic) hash used to detect whether the source
    /// has actually changed, enabling skip of redundant re-parse and re-bind.
    pub const fn content_hash(&self) -> u64 {
        self.content_hash
    }

    /// Record that this file was accessed by an LSP operation.
    ///
    /// Updates the `last_accessed` timestamp to `Instant::now()`. Called by
    /// the `Project` when the file is used for diagnostics, hover,
    /// completions, go-to-definition, references, or similar operations.
    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    /// Timestamp of the last LSP access to this file.
    ///
    /// Used by eviction heuristics to identify cold files.
    pub const fn last_accessed(&self) -> Instant {
        self.last_accessed
    }

    /// Estimate the heap memory footprint of this file in bytes.
    ///
    /// Accounts for binder state (symbols, scopes, flow nodes, hash maps) and
    /// the parser arena (nodes + typed pools, rough estimate). Used for memory
    /// pressure tracking and eviction decisions at the `Project` level.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // file_name
        size += self.file_name.capacity();

        // Parser arena: each node is 16 bytes, plus rough overhead for typed pools.
        // We use 2x the node-header footprint as a conservative estimate for the
        // pools (identifiers, literals, expressions, etc.) that accompany nodes.
        let node_count = self.parser.get_node_count();
        size += node_count * 16 * 2;

        // Source text retained by the scanner inside ParserState.
        size += self.source_text().len();

        // --- Binder state ---
        let b = &self.binder;

        // symbols
        size += b.symbols.len() * std::mem::size_of::<tsz_binder::Symbol>();
        for sym in b.symbols.iter() {
            size += sym.escaped_name.capacity();
            size += sym.declarations.capacity() * std::mem::size_of::<NodeIndex>();
            if let Some(ref exports) = sym.exports {
                size += exports.len() * (32 + std::mem::size_of::<SymbolId>());
            }
            if let Some(ref members) = sym.members {
                size += members.len() * (32 + std::mem::size_of::<SymbolId>());
            }
            if let Some(ref s) = sym.import_module {
                size += s.capacity();
            }
            if let Some(ref s) = sym.import_name {
                size += s.capacity();
            }
        }

        // file_locals
        size += b.file_locals.len() * (32 + std::mem::size_of::<SymbolId>());

        // declared_modules
        for s in &b.declared_modules {
            size += s.capacity() + 8;
        }

        // node_symbols
        size += b.node_symbols.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<SymbolId>() + 8);

        // scopes
        size += b.scopes.capacity() * std::mem::size_of::<tsz_binder::Scope>();
        for scope in &b.scopes {
            size += scope.table.len() * (32 + std::mem::size_of::<SymbolId>());
        }

        // node_scope_ids
        size += b.node_scope_ids.capacity() * (std::mem::size_of::<u32>() + 4 + 8);

        // flow_nodes
        size += b.flow_nodes.len() * std::mem::size_of::<tsz_binder::FlowNode>();
        for flow_node in b.flow_nodes.iter() {
            size += flow_node.antecedent.capacity() * std::mem::size_of::<tsz_binder::FlowNodeId>();
        }

        // node_flow
        size += b.node_flow.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<tsz_binder::FlowNodeId>() + 8);

        // switch_clause_to_switch
        size += b.switch_clause_to_switch.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<NodeIndex>() + 8);

        // symbol_arenas (Arc overhead only, shared data not counted)
        size += b.symbol_arenas.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<usize>() + 8);

        // declaration_arenas
        size +=
            b.declaration_arenas.len() * (std::mem::size_of::<(SymbolId, NodeIndex)>() + 32 + 8);

        // global_augmentations
        for (k, v) in b.global_augmentations.iter() {
            size += k.capacity() + 8;
            size += v.capacity() * std::mem::size_of::<tsz_binder::GlobalAugmentation>();
        }

        // expando_properties
        for (k, v) in b.expando_properties.iter() {
            size += k.capacity() + 8;
            for s in v {
                size += s.capacity() + 8;
            }
        }

        // line_map
        size += std::mem::size_of::<LineMap>();

        size
    }

    /// Get the strict mode setting for type checking.
    pub const fn strict(&self) -> bool {
        self.strict
    }

    /// Set the strict mode for type checking.
    pub const fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    pub fn update_source(&mut self, source_text: String) {
        self.content_hash = hash_source_content(&source_text);
        self.parser.reset(self.file_name.clone(), source_text);
        self.root = self.parser.parse_source_file();

        let arena = self.parser.get_arena();
        self.binder.reset();
        // Preserve file_idx across re-binds so the DefinitionStore can
        // track which definitions belong to this file.
        if self.file_idx != u32::MAX {
            self.binder.set_file_idx(self.file_idx);
        }
        self.binder.bind_source_file(arena, self.root);

        self.line_map = LineMap::build(self.parser.get_source_text());
        self.reset_analysis_state();
        self.export_signature = ExportSignature::compute(&self.binder, &self.file_name);
    }

    /// Invalidate all caches for this file.
    ///
    /// This should be called when a dependency of this file changes, forcing
    /// recomputation of type information and scope analysis on next access.
    pub fn invalidate_caches(&mut self) {
        self.reset_analysis_state();
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
        let source_file = arena.get_source_file_at(self.root)?;
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
            let Some(source_file) = arena.get_source_file_at(self.root) else {
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
        let new_content_hash = hash_source_content(&new_text);
        let line_map = LineMap::build(&new_text);
        let comments = tsz_common::comments::get_comment_ranges(&new_text);

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
        self.content_hash = new_content_hash;
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
            if self.file_idx != u32::MAX {
                self.binder.set_file_idx(self.file_idx);
            }
            self.binder.bind_source_file(arena, self.root);
        }
        self.reset_analysis_state();
        self.export_signature = ExportSignature::compute(&self.binder, &self.file_name);

        true
    }

    fn reset_analysis_state(&mut self) {
        // Note: the type_interner is NOT reset here. It is shared across all
        // files in a Project via Arc, and TypeInterner is append-only (interned
        // types are never removed). Resetting it would invalidate TypeIds held
        // by other files. The per-file caches (type_cache, scope_cache) are
        // invalidated to force re-computation with the shared interner.
        self.type_cache = None;
        self.scope_cache.clear();
        self.diagnostics_dirty = true;
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
        let compiler_options = tsz_checker::context::CheckerOptions {
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

        let query_cache = tsz_solver::QueryCache::new(&self.type_interner);

        let mut checker = match (self.type_cache.take(), &self.definition_store) {
            (Some(cache), Some(def_store)) => CheckerState::with_cache_and_shared_def_store(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
                file_name,
                cache,
                compiler_options,
                Arc::clone(def_store),
            ),
            (Some(cache), None) => CheckerState::with_cache(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
                file_name,
                cache,
                compiler_options,
            ),
            (None, Some(def_store)) => CheckerState::new_with_shared_def_store(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
                file_name,
                compiler_options,
                Arc::clone(def_store),
            ),
            (None, None) => CheckerState::new(
                self.parser.get_arena(),
                &self.binder,
                &query_cache,
                file_name,
                compiler_options,
            ),
        };

        checker.check_source_file(self.root);

        let diagnostics = checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| convert_diagnostic(diag, &self.line_map, source_text))
            .collect();

        self.type_cache = Some(checker.extract_cache());
        self.diagnostics_dirty = false;
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

    fn node_symbol_text(&self, node_idx: NodeIndex) -> Option<&str> {
        let arena = self.arena();
        arena
            .get_identifier_text(node_idx)
            .or_else(|| arena.get_literal_text(node_idx))
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

        let Some(source_file) = arena.get_source_file_at(self.root()) else {
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
            if export.module_specifier.is_some() {
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

        let Some(source_file) = arena.get_source_file_at(self.root()) else {
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
            if export.module_specifier.is_some() {
                continue;
            }

            if export.is_default_export {
                if export.export_clause.is_some()
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
                        let Some(spec) = arena.get_specifier_at(spec_idx) else {
                            continue;
                        };

                        let local_ident = if spec.property_name.is_some() {
                            spec.property_name
                        } else {
                            spec.name
                        };
                        if self.resolve_symbol(local_ident) != Some(sym_id) {
                            continue;
                        }

                        let export_ident = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        if let Some(export_text) = self.node_symbol_text(export_ident) {
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

        let Some(source_file) = arena.get_source_file_at(self.root()) else {
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

            let Some(clause) = arena.get_import_clause_at(import.import_clause) else {
                continue;
            };

            if clause.name.is_some()
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

            if named.name.is_some()
                && let Some(name) = arena.get_identifier_text(named.name)
                && name == local_name
            {
                targets.push(ImportTarget {
                    module_specifier: module_specifier.clone(),
                    kind: ImportKind::Namespace,
                });
            }

            for &spec_idx in &named.elements.nodes {
                let Some(spec) = arena.get_specifier_at(spec_idx) else {
                    continue;
                };

                let local_ident = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                let Some(local_text) = self.node_symbol_text(local_ident) else {
                    continue;
                };
                if local_text != local_name {
                    continue;
                }

                let export_ident = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                let Some(export_text) = self.node_symbol_text(export_ident) else {
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

        let Some(named) = arena.get_named_imports_at(clause_idx) else {
            return;
        };

        for &spec_idx in &named.elements.nodes {
            let Some(spec) = arena.get_specifier_at(spec_idx) else {
                continue;
            };

            let export_ident = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            let Some(export_text) = self.node_symbol_text(export_ident) else {
                continue;
            };
            if export_text != export_name {
                continue;
            }

            let local_ident = if spec.property_name.is_some() {
                spec.property_name
            } else {
                spec.name
            };
            if let Some(local_text) = self.node_symbol_text(local_ident) {
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
                    arena
                        .get_variable_declaration_at(decl_idx)
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
    Implementations,
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

/// Aggregate memory residency statistics for the entire `Project`.
///
/// Provides a snapshot of how much memory the project's files consume.
/// Useful for telemetry, memory pressure decisions, and eviction heuristics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectResidencyStats {
    /// Total number of files in the project.
    pub file_count: usize,
    /// Sum of `estimated_size_bytes()` across all files.
    pub total_estimated_bytes: usize,
    /// Largest single-file estimate, with its name.
    pub largest_file: Option<(String, usize)>,
    /// Smallest single-file estimate, with its name.
    pub smallest_file: Option<(String, usize)>,
    /// Estimated size of the shared `TypeInterner` in bytes.
    /// This is a project-wide cost shared across all files via `Arc`.
    pub type_interner_estimated_bytes: usize,
    /// Estimated size of the shared `DefinitionStore` in bytes.
    /// This is a project-wide cost shared across all files via `Arc`.
    pub definition_store_estimated_bytes: usize,
}

impl ProjectResidencyStats {
    /// Total estimated size in megabytes (truncated).
    #[must_use]
    pub const fn total_mb(&self) -> usize {
        self.total_estimated_bytes / (1024 * 1024)
    }
}

/// Per-file memory residency snapshot for eviction decisions.
///
/// Combines estimated heap size with access recency so callers can
/// rank files for eviction (e.g., least-recently-used + largest first).
#[derive(Debug, Clone)]
pub struct FileResidencyInfo {
    /// File name (URI / path).
    pub file_name: String,
    /// Estimated heap footprint in bytes.
    pub estimated_bytes: usize,
    /// How long ago the file was last accessed by an LSP operation.
    pub idle_duration: Duration,
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

fn normalize_auto_import_exclude_pattern(pattern: &str) -> Option<String> {
    let normalized = pattern.trim().replace('\\', "/");
    let stripped = normalized.strip_prefix("./").unwrap_or(&normalized).trim();
    (!stripped.is_empty()).then_some(stripped.to_string())
}

fn contains_glob_meta(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[') || pattern.contains(']')
}

fn should_add_recursive_exclude_variant(pattern: &str) -> bool {
    if pattern.ends_with("/**") || pattern.ends_with("/**/*") {
        return false;
    }

    let base = pattern.trim_end_matches('/');
    let last_segment = base.rsplit('/').next().unwrap_or(base);

    !last_segment.is_empty() && !contains_glob_meta(last_segment) && !last_segment.contains('.')
}

fn expand_auto_import_exclude_pattern(pattern: &str) -> Vec<String> {
    let base = pattern.trim_end_matches('/').to_string();
    if base.is_empty() {
        return Vec::new();
    }

    let mut expanded = vec![base.clone()];
    if should_add_recursive_exclude_variant(&base) {
        expanded.push(format!("{base}/**"));
    }
    expanded
}

fn parse_regex_literal_pattern(input: &str) -> Option<(&str, &str)> {
    if !input.starts_with('/') {
        return None;
    }

    let mut closing = None;
    let mut escaped = false;
    for (idx, ch) in input.char_indices().skip(1) {
        if ch == '/' && !escaped {
            closing = Some(idx);
        }
        escaped = ch == '\\' && !escaped;
    }

    let closing = closing?;

    let body = &input[1..closing];
    if body.is_empty() {
        return None;
    }

    let mut body_escaped = false;
    for ch in body.chars() {
        if ch == '/' && !body_escaped {
            return None;
        }
        body_escaped = ch == '\\' && !body_escaped;
    }

    Some((body, &input[closing + 1..]))
}

fn compile_auto_import_specifier_exclude_pattern(pattern: &str) -> Option<Regex> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return None;
    }

    if let Some((body, flags)) = parse_regex_literal_pattern(pattern) {
        let mut builder = RegexBuilder::new(body);
        for flag in flags.chars() {
            match flag {
                'i' => {
                    builder.case_insensitive(true);
                }
                'm' => {
                    builder.multi_line(true);
                }
                's' => {
                    builder.dot_matches_new_line(true);
                }
                'x' => {
                    builder.ignore_whitespace(true);
                }
                // JavaScript flags that don't affect `is_match` behavior here.
                'g' | 'y' | 'u' | 'd' => {}
                _ => return None,
            }
        }
        return builder.build().ok();
    }

    Regex::new(pattern).ok()
}

/// Multi-file container for LSP operations.
pub struct Project {
    pub(crate) files: FxHashMap<String, ProjectFile>,
    pub(crate) dependency_graph: DependencyGraph,
    pub(crate) symbol_index: SymbolIndex,
    pub(crate) performance: ProjectPerformance,
    pub(crate) strict: bool,
    pub(crate) allow_importing_ts_extensions: bool,
    pub(crate) import_module_specifier_ending: Option<String>,
    pub(crate) import_module_specifier_preference: Option<String>,
    pub(crate) auto_import_file_exclude_matchers: Vec<globset::GlobMatcher>,
    pub(crate) auto_import_specifier_exclude_matchers: Vec<Regex>,
    pub(crate) auto_imports_allowed_without_tsconfig: bool,
    /// Workspace root directories (from workspace folders or tsconfig locations).
    pub(crate) workspace_roots: Vec<String>,
    /// Parsed tsconfig.json settings per workspace root.
    pub(crate) tsconfig_settings: FxHashMap<String, TsConfigSettings>,
    /// Shared type interner for cross-file type identity.
    ///
    /// All `ProjectFile` instances in this project share the same `TypeInterner`,
    /// ensuring that `TypeId`s are globally unique across files. This is a
    /// prerequisite for wiring shared `DefinitionStore` into per-file checkers,
    /// since `DefId -> TypeId` resolution requires a single type universe.
    pub(crate) type_interner: Arc<TypeInterner>,
    /// Shared definition store for cross-file `DefId` consistency.
    ///
    /// All `CheckerState` instances created for files in this project share
    /// the same `DefinitionStore`, ensuring that `DefId`s are globally unique
    /// and cross-file type references resolve correctly.
    ///
    /// Wired into per-file checkers via `ProjectFile::definition_store` field.
    /// When a `ProjectFile` has a shared `DefinitionStore`, its `get_diagnostics()`
    /// method uses `CheckerState::with_cache_and_shared_def_store` (or
    /// `new_with_shared_def_store`) to propagate it into the checker context.
    pub(crate) definition_store: Arc<DefinitionStore>,
    /// Stable file ID allocator for per-file `DefinitionStore` invalidation.
    ///
    /// Assigns a unique `u32` to each file name, ensuring that definitions
    /// registered in the `DefinitionStore` carry stable file provenance.
    /// When a file is removed or replaced, `invalidate_file(file_idx)` cleans
    /// up all stale definitions.
    pub(crate) file_id_allocator: FileIdAllocator,
    /// Centralized export signature fingerprint cache.
    ///
    /// Tracks the most recent export signature (as a `u64` fingerprint) for
    /// every file in the project, keyed by `file_idx`. Updated on every
    /// `set_file`/`update_file`/`remove_file` call.
    ///
    /// Enables batch change detection via
    /// [`tsz_solver::def::incremental::diff_fingerprints`] — snapshot before
    /// and after a batch of edits, diff the snapshots, and apply invalidation
    /// in one pass.
    pub(crate) fingerprint_cache: SkeletonFingerprintCache,
    /// Files currently open in the editor (tracked via `didOpen`/`didClose`).
    ///
    /// Open files are never evicted under memory pressure. The eviction module
    /// uses this set to skip actively-edited files.
    pub(crate) open_files: FxHashSet<String>,
}

/// Assigns stable `u32` file indices to file names.
///
/// Each file name gets a unique, monotonically increasing ID. IDs are never
/// reused (even after file removal) to avoid ABA problems where a new file
/// might accidentally inherit stale definitions from an old file with the
/// same ID.
///
/// The allocator is O(1) for both allocation and lookup.
#[derive(Debug, Clone, Default)]
pub(crate) struct FileIdAllocator {
    /// Maps file name to its assigned `u32` file index.
    name_to_id: FxHashMap<String, u32>,
    /// Reverse mapping: file index -> file name.
    /// Indexed by the `u32` file index. Entries are set to empty string on removal
    /// (IDs are never recycled, so the slot stays allocated).
    id_to_name: Vec<String>,
    /// Next ID to allocate.
    next_id: u32,
}

impl FileIdAllocator {
    /// Create a new allocator.
    pub fn new() -> Self {
        Self {
            name_to_id: FxHashMap::default(),
            id_to_name: Vec::new(),
            // Start at 0; u32::MAX is reserved as "unassigned".
            next_id: 0,
        }
    }

    /// Get or allocate a stable file index for the given file name.
    ///
    /// If the file already has an ID, returns it. Otherwise, allocates a new
    /// one. IDs are never reused.
    pub fn get_or_allocate(&mut self, file_name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(file_name) {
            return id;
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("file ID overflow");
        self.name_to_id.insert(file_name.to_string(), id);
        self.id_to_name.push(file_name.to_string());
        debug_assert_eq!(self.id_to_name.len(), id as usize + 1);
        id
    }

    /// Look up the file index for a file name without allocating.
    ///
    /// Returns `None` if the file was never registered.
    pub fn lookup(&self, file_name: &str) -> Option<u32> {
        self.name_to_id.get(file_name).copied()
    }

    /// Remove a file name from the allocator.
    ///
    /// The ID is NOT recycled — future allocations continue from `next_id`.
    /// This prevents stale definition collisions.
    pub fn remove(&mut self, file_name: &str) -> Option<u32> {
        let id = self.name_to_id.remove(file_name)?;
        // Clear the reverse entry. The slot stays allocated (IDs are never recycled).
        if let Some(entry) = self.id_to_name.get_mut(id as usize) {
            entry.clear();
        }
        Some(id)
    }

    /// Look up the file name for a given file index.
    ///
    /// Returns `None` if the index was never allocated or the file was removed.
    pub fn name_for_id(&self, file_idx: u32) -> Option<&str> {
        let name = self.id_to_name.get(file_idx as usize)?;
        if name.is_empty() {
            None
        } else {
            Some(name.as_str())
        }
    }

    /// Number of currently tracked files.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.name_to_id.len()
    }
}

/// Centralized cache of per-file export signature fingerprints.
///
/// Maintains a `file_idx -> fingerprint` mapping that tracks the most recent
/// export signature for every file in the project. This enables:
///
/// 1. **O(1) change detection**: compare old and new fingerprints to determine
///    whether a file's public API changed.
/// 2. **Batch diffing**: snapshot the cache as `(file_idx, fingerprint)` pairs
///    and feed them to [`tsz_solver::def::incremental::diff_fingerprints`] for
///    coordinated multi-file invalidation.
/// 3. **Separation of concerns**: the `Project` stores fingerprints in one
///    central location rather than scattering them across `ProjectFile` fields.
///
/// The cache is updated on every `set_file`, `update_file`, and `remove_file`
/// call. It is read-only during diagnostic computation.
#[derive(Debug, Clone, Default)]
pub(crate) struct SkeletonFingerprintCache {
    /// Maps `file_idx` to the file's current export signature fingerprint.
    entries: FxHashMap<u32, u64>,
}

impl SkeletonFingerprintCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }

    /// Record or update the fingerprint for a file.
    ///
    /// Returns the previous fingerprint if the file was already tracked,
    /// or `None` if this is a new entry.
    pub fn update(&mut self, file_idx: u32, fingerprint: u64) -> Option<u64> {
        self.entries.insert(file_idx, fingerprint)
    }

    /// Remove a file's fingerprint from the cache.
    ///
    /// Returns the removed fingerprint, or `None` if the file was not tracked.
    pub fn remove(&mut self, file_idx: u32) -> Option<u64> {
        self.entries.remove(&file_idx)
    }

    /// Look up the current fingerprint for a file.
    pub fn get(&self, file_idx: u32) -> Option<u64> {
        self.entries.get(&file_idx).copied()
    }

    /// Snapshot all entries as `(file_idx, fingerprint)` pairs.
    ///
    /// The output is suitable for [`tsz_solver::def::incremental::diff_fingerprints`].
    pub fn snapshot(&self) -> Vec<(u32, u64)> {
        self.entries.iter().map(|(&k, &v)| (k, v)).collect()
    }

    /// Number of files tracked.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Parsed settings from tsconfig.json relevant to LSP operation.
#[derive(Debug, Clone, Default)]
pub struct TsConfigSettings {
    /// The root directory containing the tsconfig.json.
    pub root_dir: String,
    /// Whether strict mode is enabled.
    pub strict: Option<bool>,
    /// Target ES version (affects lib files).
    pub target: Option<String>,
    /// Module resolution strategy.
    pub module_resolution: Option<String>,
    /// Base URL for module resolution.
    pub base_url: Option<String>,
    /// Path mappings for module resolution.
    pub paths: FxHashMap<String, Vec<String>>,
    /// Files to include.
    pub include: Vec<String>,
    /// Files to exclude.
    pub exclude: Vec<String>,
    /// Root directory for source files.
    pub root_dir_setting: Option<String>,
    /// Output directory.
    pub out_dir: Option<String>,
    /// Whether to allow importing .ts extensions.
    pub allow_importing_ts_extensions: Option<bool>,
    /// JSX setting.
    pub jsx: Option<String>,
}

impl Project {
    /// Create a new empty project.
    pub fn new() -> Self {
        Self {
            files: FxHashMap::default(),
            dependency_graph: DependencyGraph::new(),
            symbol_index: SymbolIndex::new(),
            performance: ProjectPerformance::default(),
            strict: false,
            allow_importing_ts_extensions: false,
            import_module_specifier_ending: None,
            import_module_specifier_preference: None,
            auto_import_file_exclude_matchers: Vec::new(),
            auto_import_specifier_exclude_matchers: Vec::new(),
            auto_imports_allowed_without_tsconfig: true,
            workspace_roots: Vec::new(),
            tsconfig_settings: FxHashMap::default(),
            type_interner: Arc::new(TypeInterner::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            file_id_allocator: FileIdAllocator::new(),
            fingerprint_cache: SkeletonFingerprintCache::new(),
            open_files: FxHashSet::default(),
        }
    }

    /// Creates an empty project using default values.
    fn empty() -> Self {
        Self {
            files: FxHashMap::default(),
            dependency_graph: DependencyGraph::new(),
            symbol_index: SymbolIndex::new(),
            performance: ProjectPerformance::default(),
            strict: false,
            allow_importing_ts_extensions: false,
            import_module_specifier_ending: None,
            import_module_specifier_preference: None,
            auto_import_file_exclude_matchers: Vec::new(),
            auto_import_specifier_exclude_matchers: Vec::new(),
            auto_imports_allowed_without_tsconfig: true,
            workspace_roots: Vec::new(),
            tsconfig_settings: FxHashMap::default(),
            type_interner: Arc::new(TypeInterner::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            file_id_allocator: FileIdAllocator::new(),
            fingerprint_cache: SkeletonFingerprintCache::new(),
            open_files: FxHashSet::default(),
        }
    }

    /// Add a workspace root directory.
    pub fn add_workspace_root(&mut self, root: String) {
        if !self.workspace_roots.contains(&root) {
            self.workspace_roots.push(root);
        }
    }

    /// Remove a workspace root directory.
    pub fn remove_workspace_root(&mut self, root: &str) {
        self.workspace_roots.retain(|r| r != root);
        self.tsconfig_settings.remove(root);
    }

    /// Get the workspace roots.
    pub fn workspace_roots(&self) -> &[String] {
        &self.workspace_roots
    }

    /// Get tsconfig settings for a workspace root.
    pub fn tsconfig_for_root(&self, root: &str) -> Option<&TsConfigSettings> {
        self.tsconfig_settings.get(root)
    }

    /// Get the shared type interner for this project.
    ///
    /// Returns a clone of the `Arc`, allowing callers to share the interner
    /// with checker instances or other components that need cross-file
    /// type identity.
    pub fn type_interner(&self) -> Arc<TypeInterner> {
        Arc::clone(&self.type_interner)
    }

    /// Get the shared definition store for this project.
    ///
    /// Returns a clone of the `Arc`, allowing callers to share the store
    /// with checker instances or other components that need cross-file
    /// `DefId` consistency.
    pub fn definition_store(&self) -> Arc<DefinitionStore> {
        Arc::clone(&self.definition_store)
    }

    /// Snapshot the current export signature fingerprints for all files.
    ///
    /// Returns `(file_idx, fingerprint)` pairs suitable for feeding to
    /// [`tsz_solver::def::incremental::diff_fingerprints`]. Take a snapshot
    /// before a batch of edits, apply the edits, take another snapshot, and
    /// diff them to determine which files' public APIs changed.
    pub fn fingerprint_snapshot(&self) -> Vec<(u32, u64)> {
        self.fingerprint_cache.snapshot()
    }

    /// Look up the current export signature fingerprint for a file.
    ///
    /// Returns `None` if the file is not in the project or has no assigned
    /// file index.
    pub fn fingerprint_for_file(&self, file_name: &str) -> Option<u64> {
        let file_idx = self.file_id_allocator.lookup(file_name)?;
        self.fingerprint_cache.get(file_idx)
    }

    /// Load and parse a tsconfig.json file, storing settings for the workspace root.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_tsconfig(&mut self, root: &str) {
        let tsconfig_path = Path::new(root).join("tsconfig.json");
        if !tsconfig_path.exists() {
            // Try jsconfig.json as fallback
            let jsconfig_path = Path::new(root).join("jsconfig.json");
            if jsconfig_path.exists()
                && let Some(settings) = parse_tsconfig_file(&jsconfig_path)
            {
                self.apply_tsconfig_settings(root, settings);
            }
            return;
        }

        if let Some(settings) = parse_tsconfig_file(&tsconfig_path) {
            self.apply_tsconfig_settings(root, settings);
        }
    }

    /// Apply parsed tsconfig settings to the project.
    #[cfg(not(target_arch = "wasm32"))]
    fn apply_tsconfig_settings(&mut self, root: &str, settings: TsConfigSettings) {
        // Apply strict mode
        if let Some(strict) = settings.strict {
            self.set_strict(strict);
        }

        // Apply allowImportingTsExtensions
        if let Some(allow) = settings.allow_importing_ts_extensions {
            self.set_allow_importing_ts_extensions(allow);
        }

        self.tsconfig_settings.insert(root.to_string(), settings);
    }

    /// Discover and load TypeScript/JavaScript files from workspace roots.
    ///
    /// Walks each workspace root directory and loads files matching common
    /// TypeScript/JavaScript extensions (.ts, .tsx, .js, .jsx, .mts, .cts).
    /// Respects tsconfig include/exclude patterns when available.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn discover_files(&mut self, roots: &[String]) -> Vec<String> {
        let mut discovered = Vec::new();

        for root in roots {
            let root_path = Path::new(root);
            if !root_path.is_dir() {
                continue;
            }

            // Get include/exclude patterns from tsconfig if available
            let (includes, excludes) = self
                .tsconfig_settings
                .get(root)
                .map(|ts| (ts.include.clone(), ts.exclude.clone()))
                .unwrap_or_else(|| {
                    (
                        vec!["**/*.ts".to_string(), "**/*.tsx".to_string()],
                        vec![
                            "node_modules".to_string(),
                            "dist".to_string(),
                            "build".to_string(),
                            ".git".to_string(),
                        ],
                    )
                });

            // Build exclude matchers
            let exclude_matchers: Vec<globset::GlobMatcher> = excludes
                .iter()
                .filter_map(|pattern| {
                    Glob::new(&format!("**/{pattern}/**"))
                        .ok()
                        .map(|g| g.compile_matcher())
                })
                .collect();

            // Walk directory
            let walker = walkdir::WalkDir::new(root_path)
                .follow_links(false)
                .max_depth(20);

            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();

                // Skip directories that match exclude patterns
                if entry.file_type().is_dir() {
                    continue;
                }

                let path_str = path.to_string_lossy().to_string();

                // Check exclude patterns
                if exclude_matchers.iter().any(|m| m.is_match(&path_str)) {
                    continue;
                }

                // Check if it's a TS/JS file
                if !is_ts_js_file(&path_str) {
                    continue;
                }

                // Only load files within include patterns if specified
                if !includes.is_empty() {
                    let _relative = path
                        .strip_prefix(root_path)
                        .unwrap_or(path)
                        .to_string_lossy();
                    // For now, load all TS/JS files (include pattern matching is complex)
                }

                // Load the file
                if let Ok(content) = std::fs::read_to_string(path) {
                    self.set_file(path_str.clone(), content);
                    discovered.push(path_str);
                }
            }
        }

        discovered
    }

    /// Get the strict mode setting for type checking.
    pub const fn strict(&self) -> bool {
        self.strict
    }

    /// Set the strict mode directly.
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
        // Update strict mode on all existing files
        for file in self.files.values_mut() {
            file.set_strict(strict);
        }
    }

    pub const fn set_allow_importing_ts_extensions(&mut self, allow: bool) {
        self.allow_importing_ts_extensions = allow;
    }

    /// Set completion module-specifier ending preference (e.g. "js").
    pub fn set_import_module_specifier_ending(&mut self, ending: Option<String>) {
        self.import_module_specifier_ending = ending;
    }

    /// Set preference for module specifier generation.
    pub fn set_import_module_specifier_preference(&mut self, pref: Option<String>) {
        self.import_module_specifier_preference = pref;
    }

    /// Set auto-import exclusion patterns used by completions and import fixes.
    pub fn set_auto_import_file_exclude_patterns(&mut self, patterns: Vec<String>) {
        self.auto_import_file_exclude_matchers.clear();
        for pattern in patterns {
            let Some(normalized) = normalize_auto_import_exclude_pattern(&pattern) else {
                continue;
            };
            for expanded in expand_auto_import_exclude_pattern(&normalized) {
                let Ok(glob) = Glob::new(&expanded) else {
                    continue;
                };
                self.auto_import_file_exclude_matchers
                    .push(glob.compile_matcher());
            }
        }
    }

    /// Set module-specifier exclusion regexes used by completions and import fixes.
    pub fn set_auto_import_specifier_exclude_regexes(&mut self, patterns: Vec<String>) {
        self.auto_import_specifier_exclude_matchers.clear();
        for pattern in patterns {
            if let Some(regex) = compile_auto_import_specifier_exclude_pattern(&pattern) {
                self.auto_import_specifier_exclude_matchers.push(regex);
            }
        }
    }

    /// Set inferred-project fallback for whether module-export auto-imports are legal.
    pub const fn set_auto_imports_allowed_without_tsconfig(&mut self, allow: bool) {
        self.auto_imports_allowed_without_tsconfig = allow;
    }

    /// Total number of files tracked by the project.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Iterate over all file names in the project.
    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(|s| s.as_str())
    }

    /// Look up the file name for a given `file_idx` (as stamped on binder symbols).
    ///
    /// Returns `None` if the index was never allocated or the file was removed.
    /// This enables resolving a symbol's owning file from its `decl_file_idx`.
    pub fn file_name_for_idx(&self, file_idx: u32) -> Option<&str> {
        self.file_id_allocator.name_for_id(file_idx)
    }

    /// Get the set of files that directly import the given file.
    pub fn get_file_dependents(&self, file: &str) -> Vec<String> {
        self.dependency_graph
            .get_dependents(file)
            .map(|deps| deps.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Snapshot of per-request timing data.
    pub const fn performance(&self) -> &ProjectPerformance {
        &self.performance
    }

    /// Compute aggregate memory residency statistics for the project.
    ///
    /// Iterates all files and sums their `estimated_size_bytes()`.
    /// The result is a snapshot — it does not cache or persist.
    #[must_use]
    pub fn residency_stats(&self) -> ProjectResidencyStats {
        let mut total: usize = 0;
        let mut largest: Option<(&str, usize)> = None;
        let mut smallest: Option<(&str, usize)> = None;

        for (name, file) in &self.files {
            let est = file.estimated_size_bytes();
            total = total.saturating_add(est);
            if largest.is_none_or(|(_, s)| est > s) {
                largest = Some((name.as_str(), est));
            }
            if smallest.is_none_or(|(_, s)| est < s) {
                smallest = Some((name.as_str(), est));
            }
        }

        ProjectResidencyStats {
            file_count: self.files.len(),
            total_estimated_bytes: total,
            largest_file: largest.map(|(n, s)| (n.to_string(), s)),
            smallest_file: smallest.map(|(n, s)| (n.to_string(), s)),
            type_interner_estimated_bytes: self.type_interner.estimated_size_bytes(),
            definition_store_estimated_bytes: self.definition_store.estimated_size_bytes(),
        }
    }

    /// Estimated memory footprint of a single file, or `None` if not tracked.
    #[must_use]
    pub fn file_estimated_size(&self, file_name: &str) -> Option<usize> {
        self.files.get(file_name).map(|f| f.estimated_size_bytes())
    }

    /// Return per-file residency info sorted for eviction (best candidates first).
    ///
    /// Files are ranked by a composite score: idle duration (seconds) multiplied
    /// by estimated size (bytes). This prefers evicting files that are both large
    /// and cold. Declaration files (`*.d.ts`) are deprioritized since they are
    /// typically shared dependencies.
    ///
    /// The optional `min_idle` parameter filters out files that have been accessed
    /// more recently than the threshold — active files are never eviction candidates.
    #[must_use]
    pub fn eviction_candidates(&self, min_idle: Option<Duration>) -> Vec<FileResidencyInfo> {
        let now = Instant::now();
        let mut candidates: Vec<FileResidencyInfo> = self
            .files
            .iter()
            .filter_map(|(name, file)| {
                let idle = now.duration_since(file.last_accessed);
                if let Some(threshold) = min_idle
                    && idle < threshold
                {
                    return None;
                }
                Some(FileResidencyInfo {
                    file_name: name.clone(),
                    estimated_bytes: file.estimated_size_bytes(),
                    idle_duration: idle,
                })
            })
            .collect();

        // Sort by composite eviction score: idle_seconds * size_bytes (descending).
        // Declaration files get a 4x penalty (lower effective score) to keep them
        // resident longer since they're shared across many importers.
        candidates.sort_by(|a, b| {
            let score = |info: &FileResidencyInfo| -> u64 {
                let idle_secs = info.idle_duration.as_secs().max(1);
                let size = info.estimated_bytes as u64;
                let raw = idle_secs.saturating_mul(size);
                if info.file_name.ends_with(".d.ts") {
                    raw / 4
                } else {
                    raw
                }
            };
            score(b).cmp(&score(a))
        });

        candidates
    }

    /// Mark a file as recently accessed.
    ///
    /// Call this when the file is used for any LSP operation (diagnostics,
    /// hover, completions, go-to-definition, references, etc.) so that
    /// eviction heuristics can distinguish hot files from cold ones.
    pub fn touch_file(&mut self, file_name: &str) {
        if let Some(file) = self.files.get_mut(file_name) {
            file.touch();
        }
    }

    /// Add or replace a file, re-parsing and re-binding its contents.
    ///
    /// If the file already exists with identical content (same content hash),
    /// the re-parse and re-bind are skipped entirely. This avoids redundant work
    /// when the LSP receives `didOpen` for an already-loaded file, or `didSave`
    /// without content changes.
    pub fn set_file(&mut self, file_name: String, source_text: String) {
        // Fast path: skip re-parse if file exists with identical content.
        let new_hash = hash_source_content(&source_text);
        if let Some(existing) = self.files.get(&file_name)
            && existing.content_hash == new_hash
        {
            return;
        }

        // Allocate a stable file index. If the file already has one, reuse it
        // (the allocator returns the existing ID). This ensures that
        // invalidate_file + re-register uses the same ID.
        let file_idx = self.file_id_allocator.get_or_allocate(&file_name);

        // Invalidate old definitions in the DefinitionStore before re-binding.
        // This cleans up stale DefIds from the previous version of this file.
        if self.files.contains_key(&file_name) {
            self.definition_store.invalidate_file(file_idx);
        }

        let file = ProjectFile::with_full_project_context(
            file_name.clone(),
            source_text,
            self.strict,
            Arc::clone(&self.type_interner),
            Arc::clone(&self.definition_store),
            file_idx,
        );

        // Update symbol index with the new file's binder data and AST identifiers
        // We need to get the arena before moving the file into self.files
        let arena = file.parser.get_arena();
        let source = file.source_text();
        self.symbol_index
            .index_file(&file_name, &file.binder, arena, source);

        // Record the new export signature in the fingerprint cache.
        let new_fp = file.export_signature.0;
        self.fingerprint_cache.update(file_idx, new_fp);

        self.files.insert(file_name.clone(), file);

        // Log per-file memory estimate for telemetry / pressure tracking.
        if let Some(f) = self.files.get(&file_name) {
            let est = f.estimated_size_bytes();
            tracing::debug!(
                file = %file_name,
                estimated_bytes = est,
                file_count = self.files.len(),
                "project: file added/replaced"
            );
        }

        // Update dependency graph with imports from this file
        self.update_dependencies(&file_name);
    }

    /// Update an existing file by applying incremental text edits.
    ///
    /// Uses export signature comparison to avoid unnecessary cache invalidation:
    /// if the file's public API (exports, re-exports, augmentations) didn't change,
    /// dependent files keep their cached diagnostics.
    ///
    /// Returns an `InvalidationSummary` describing what changed and how many
    /// dependents were invalidated. Useful for perf analysis.
    pub fn update_file(
        &mut self,
        file_name: &str,
        edits: &[TextEdit],
    ) -> Option<InvalidationSummary> {
        if edits.is_empty() {
            let sig = self.files.get(file_name)?.export_signature.0;
            return Some(InvalidationSummary::unchanged(file_name.to_string(), sig));
        }

        let (updated_source, unchanged) = {
            let file = self.files.get(file_name)?;
            let source = file.source_text();
            let updated = apply_text_edits(source, file.line_map(), edits)?;
            let unchanged = updated == source;
            (updated, unchanged)
        };

        if unchanged {
            let sig = self.files.get(file_name)?.export_signature.0;
            return Some(InvalidationSummary::unchanged(file_name.to_string(), sig));
        }

        // Capture old export signature before updating
        let old_signature = self.files.get(file_name)?.export_signature;

        // Invalidate old definitions before re-binding. The re-bind will
        // create new definitions with the same file_idx.
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.definition_store.invalidate_file(file_idx);
        }

        let file = self.files.get_mut(file_name)?;
        file.update_source_with_edits(updated_source, edits);

        // Re-index the file in the symbol index with updated binder and arena
        let arena = file.parser.get_arena();
        let source = file.source_text();
        self.symbol_index
            .update_file(file_name, &file.binder, arena, source);

        // Update the fingerprint cache with the new export signature.
        let new_signature = file.export_signature;
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.fingerprint_cache.update(file_idx, new_signature.0);
        }

        // Smart cache invalidation: only invalidate dependents if the public API changed.
        // Body-only edits, comment changes, and private symbol changes won't trigger
        // dependent re-checking — this is the key optimization.
        if old_signature != new_signature {
            let affected_files = self.dependency_graph.get_affected_files(file_name);
            let mut invalidated_count = 0;
            for affected_file in affected_files {
                if let Some(dep_file) = self.files.get_mut(&affected_file) {
                    dep_file.invalidate_caches();
                    invalidated_count += 1;
                }
            }
            Some(InvalidationSummary::changed(
                file_name.to_string(),
                Some(old_signature.0),
                new_signature.0,
                invalidated_count,
            ))
        } else {
            Some(InvalidationSummary::unchanged(
                file_name.to_string(),
                new_signature.0,
            ))
        }
    }

    /// Remove a file from the project.
    ///
    /// Cleans up:
    /// - Stale definitions in the shared `DefinitionStore`
    /// - Symbol index entries for the file
    /// - Dependency graph edges (both imports and dependents)
    /// - Cached diagnostics/types in files that depended on the removed file
    /// - File ID allocation (ID is retired, not recycled)
    pub fn remove_file(&mut self, file_name: &str) -> Option<ProjectFile> {
        // Invalidate definitions in the DefinitionStore for this file.
        // This must happen before removing from the files map so the file_idx
        // is still available.
        if let Some(file_idx) = self.file_id_allocator.lookup(file_name) {
            self.definition_store.invalidate_file(file_idx);
            // Remove from fingerprint cache.
            self.fingerprint_cache.remove(file_idx);
        }
        // Remove the file ID (retired, not recycled).
        self.file_id_allocator.remove(file_name);

        // Remove from symbol index
        self.symbol_index.remove_file(file_name);

        // Invalidate caches in files that depend on the removed file,
        // since the removed file's exports are no longer available.
        let affected_files = self.dependency_graph.get_affected_files(file_name);
        for affected_file in affected_files {
            if let Some(dep_file) = self.files.get_mut(&affected_file) {
                dep_file.invalidate_caches();
            }
        }

        // Remove from dependency graph (cleans up both outgoing and incoming edges)
        self.dependency_graph.remove_file(file_name);

        // Log memory freed by removal.
        let freed_bytes = self
            .files
            .get(file_name)
            .map(|f| f.estimated_size_bytes())
            .unwrap_or(0);

        let removed = self.files.remove(file_name);

        if removed.is_some() {
            tracing::debug!(
                file = %file_name,
                freed_bytes,
                remaining_files = self.files.len(),
                "project: file removed"
            );
        }

        removed
    }

    /// Update the dependency graph for a file using binder-collected import sources.
    ///
    /// Uses `BinderState::file_import_sources` which the binder populates during
    /// binding from static import/export declarations. This avoids a redundant
    /// full-AST walk that the previous `extract_imports` method performed.
    ///
    /// Note: `file_import_sources` captures static imports only (import/export
    /// declarations, `import = require()`). Dynamic `import()` and `require()`
    /// calls are not included, which is the correct behavior for the dependency
    /// graph — dynamic imports are lazy and should not trigger eager invalidation.
    fn update_dependencies(&mut self, file_name: &str) {
        let imports = match self.files.get(file_name) {
            Some(file) => file.binder.file_import_sources.clone(),
            None => return,
        };
        self.dependency_graph.update_file(file_name, &imports);
    }

    /// Handle file rename requests from the LSP client.
    ///
    /// When files are renamed or moved, this calculates the `TextEdits` needed
    /// to update import statements in all dependent files.
    ///
    /// # Arguments
    /// * `renames` - List of file renames (old path -> new path)
    ///
    /// # Returns
    /// A `WorkspaceEdit` containing all the `TextEdits` needed to update imports
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
    #[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(not(target_arch = "wasm32"))]
    fn process_file_rename(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        result: &mut WorkspaceEdit,
    ) {
        use crate::rename::file_rename::FileRenameProvider;
        use crate::utils::calculate_new_relative_path;
        use std::path::Path;

        // Iterate through all files to find those that import the renamed file
        // We can't use dependency_graph.get_dependents() directly because it stores
        // raw import specifiers (e.g., "./utils/math") not resolved file paths
        for (dependent_path, dep_file) in &self.files {
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
                    // `import_loc.range` spans the surrounding quotes; use the
                    // helper so the rewrite replaces only the inner content
                    // and the original quote style is preserved.
                    result.add_edit(
                        dependent_path.clone(),
                        import_loc.specifier_text_edit(new_specifier),
                    );
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
    #[cfg(not(target_arch = "wasm32"))]
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
        if let Some(target_stem) = target.file_stem()
            && let Some(resolved_stem) = normalized_path.file_stem()
            && target_stem == resolved_stem
        {
            // Normalize target as well for comparison
            let normalized_target = self.normalize_path(target);
            let normalized_target_path = Path::new(&normalized_target);
            // Check if parent dirs match
            if normalized_path.parent() == normalized_target_path.parent() {
                return true;
            }
        }

        false
    }

    /// Simple path normalization that resolves . and .. components without filesystem access.
    #[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(not(target_arch = "wasm32"))]
    fn is_directory(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let path_str_ref = path_str.as_ref();

        // Check if any file in the project has this path as a prefix
        for file_path in self.files.keys() {
            if file_path.starts_with(path_str_ref) {
                // Ensure it's a proper directory separator
                let Some(rest) = file_path.strip_prefix(path_str_ref) else {
                    continue;
                };
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
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Get candidate files that might contain references to a symbol.
    ///
    /// This uses the `SymbolIndex` for O(1) lookup, turning cross-file searches
    /// from O(N) where N = all files to O(M) where M = files containing the symbol.
    ///
    /// # Arguments
    /// * `symbol_name` - The symbol name to search for
    ///
    /// # Returns
    /// A list of file paths that contain references to the symbol.
    /// Falls back to all files if the index is empty (e.g., for wildcard re-exports).
    pub(crate) fn get_candidate_files_for_symbol(&self, symbol_name: &str) -> Vec<String> {
        let candidate_files = self.symbol_index.get_files_with_symbol(symbol_name);
        if candidate_files.is_empty() {
            // Fallback to all files if index is empty
            // This handles wildcard re-exports (export * from './mod')
            self.files.keys().cloned().collect()
        } else {
            candidate_files.into_iter().collect()
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

/// Check whether a file path has a TypeScript/JavaScript extension.
#[cfg(not(target_arch = "wasm32"))]
fn is_ts_js_file(path: &str) -> bool {
    let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
    extensions.iter().any(|ext| path.ends_with(ext))
}

/// Parse a tsconfig.json or jsconfig.json file into `TsConfigSettings`.
#[cfg(not(target_arch = "wasm32"))]
fn parse_tsconfig_file(path: &std::path::Path) -> Option<TsConfigSettings> {
    let content = std::fs::read_to_string(path).ok()?;

    // Use json5 parser to handle comments and trailing commas
    let value: serde_json::Value = json5::from_str(&content).ok()?;
    let obj = value.as_object()?;

    let root_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut settings = TsConfigSettings {
        root_dir,
        ..Default::default()
    };

    // Parse compilerOptions
    if let Some(compiler_options) = obj.get("compilerOptions").and_then(|v| v.as_object()) {
        settings.strict = compiler_options.get("strict").and_then(|v| v.as_bool());

        settings.target = compiler_options
            .get("target")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.module_resolution = compiler_options
            .get("moduleResolution")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.base_url = compiler_options
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.root_dir_setting = compiler_options
            .get("rootDir")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.out_dir = compiler_options
            .get("outDir")
            .and_then(|v| v.as_str())
            .map(String::from);

        settings.allow_importing_ts_extensions = compiler_options
            .get("allowImportingTsExtensions")
            .and_then(|v| v.as_bool());

        settings.jsx = compiler_options
            .get("jsx")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Parse paths
        if let Some(paths) = compiler_options.get("paths").and_then(|v| v.as_object()) {
            for (key, val) in paths {
                if let Some(arr) = val.as_array() {
                    let mapped: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    settings.paths.insert(key.clone(), mapped);
                }
            }
        }
    }

    // Parse include
    if let Some(include) = obj.get("include").and_then(|v| v.as_array()) {
        settings.include = include
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }

    // Parse exclude
    if let Some(exclude) = obj.get("exclude").and_then(|v| v.as_array()) {
        settings.exclude = exclude
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }

    Some(settings)
}
