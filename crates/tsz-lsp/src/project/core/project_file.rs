use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHasher;
use web_time::Instant;

use super::{ImportKind, ImportTarget};
use crate::completions::{CompletionItem, Completions};
use crate::diagnostics::{LspDiagnostic, convert_diagnostic};
use crate::export_signature::ExportSignature;
use crate::hover::{HoverInfo, HoverProvider};
use crate::project::LspProviderContext;
use crate::rename::TextEdit;
use crate::resolver::{ScopeCache, ScopeCacheStats, scope_cache_estimated_size_bytes};
use crate::signature_help::{SignatureHelp, SignatureHelpProvider};
use tsz_binder::lib_loader::LibFile;
use tsz_binder::{BinderState, SymbolId};
use tsz_checker::TypeCache;
use tsz_checker::context::LibContext;
use tsz_checker::state::CheckerState;
use tsz_common::position::{LineMap, Location, Position, Range};
use tsz_parser::ParserState;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::{NodeArena, NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::construction::TypeInterner;
use tsz_solver::def::DefinitionStore;

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
    /// Cached result of scanning statements for wildcard re-export patterns.
    ///
    /// Computed once at parse time and used by auto-import candidate collection
    /// to avoid re-scanning every file's statements on each request.
    pub(crate) has_wildcard_reexport: bool,
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
    /// Standard-library files (parsed and bound) shared from the owning
    /// `Project`. Empty in standalone mode (no `Project`), in which case the
    /// file is bound and checked without any global lib symbols — matching the
    /// historical behavior. When non-empty, the lib symbols are merged into the
    /// per-file binder (so global values like `Date`/`Map`/`Math` resolve), the
    /// checker is seeded via `set_lib_contexts`, and the hover/completion/
    /// signature providers receive the same contexts.
    pub(crate) lib_files: Arc<Vec<Arc<LibFile>>>,
    /// Flag indicating if caches were invalidated and diagnostics need re-computation
    pub(crate) diagnostics_dirty: bool,
    /// Diagnostics from the most recent full recompute, served by the
    /// pull-model when still valid instead of re-running `check_source_file`.
    ///
    /// Cleared by `reset_analysis_state` (own edits, dependency invalidation).
    /// Within a `Project`, validity additionally requires the stamp in
    /// `diagnostics_generation` to match the project-wide generation, which is
    /// the coarse cross-file invalidation barrier.
    pub(crate) cached_diagnostics: Option<Vec<LspDiagnostic>>,
    /// Pull-model result identity of `cached_diagnostics`.
    ///
    /// Assigned by the owning `Project` from a monotonically increasing
    /// sequence on every recompute; never reused, so a client-provided
    /// `previousResultId` can only match the exact recompute that produced it.
    pub(crate) diagnostics_result_id: Option<String>,
    /// Project diagnostics generation observed when `cached_diagnostics` was
    /// computed. The owning `Project` bumps its generation on every mutation
    /// that could affect any file's diagnostics (content change, file
    /// add/remove, compiler-option change); a mismatched stamp marks the
    /// cache stale.
    pub(crate) diagnostics_generation: u64,
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
pub(super) fn hash_source_content(source: &str) -> u64 {
    let mut hasher = FxHasher::default();
    source.hash(&mut hasher);
    hasher.finish()
}

/// Bind a source file, merging standard-library global symbols when any lib
/// files are present.
///
/// With no lib files this is exactly `BinderState::bind_source_file`, preserving
/// the historical standalone behavior. With lib files it routes through
/// `bind_source_file_with_libs`, which merges the lib symbols (remapping
/// `SymbolId`s to avoid collisions) so global values such as `Date`, `Map`, and
/// `Math` resolve in the per-file binder.
fn bind_with_optional_libs(
    binder: &mut BinderState,
    arena: &NodeArena,
    root: NodeIndex,
    lib_files: &[Arc<LibFile>],
) {
    if lib_files.is_empty() {
        binder.bind_source_file(arena, root);
    } else {
        binder.bind_source_file_with_libs(arena, root, lib_files);
    }
}

/// Build the checker options used for both diagnostics and the type-aware
/// providers, keyed off the file's `strict` flag.
///
/// Centralized so the hover/completion/signature providers check under the same
/// options as `compute_diagnostics`.
fn checker_options_for(strict: bool) -> tsz_checker::context::CheckerOptions {
    tsz_checker::context::CheckerOptions {
        strict,
        no_implicit_any: strict,
        no_implicit_returns: false,
        no_implicit_this: strict,
        strict_null_checks: strict,
        strict_function_types: strict,
        strict_property_initialization: strict,
        use_unknown_in_catch_variables: strict,
        isolated_modules: false,
        ..Default::default()
    }
}

/// Scan the top-level statements of a source file and return `true` if any
/// export declaration looks like a wildcard or default re-export.
///
/// Called once per file at parse time; the result is stored in
/// `ProjectFile::has_wildcard_reexport` so that auto-import candidate
/// collection can avoid re-scanning every file's AST on each request.
pub(crate) fn compute_has_wildcard_reexport(arena: &NodeArena, root: NodeIndex) -> bool {
    let Some(source_file) = arena.get_source_file_at(root) else {
        return false;
    };

    source_file.statements.nodes.iter().any(|&stmt_idx| {
        let Some(stmt_node) = arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
            return arena
                .get_export_assignment(stmt_node)
                .is_some_and(|assign| !assign.is_export_equals);
        }
        if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return false;
        }
        let Some(export) = arena.get_export_decl(stmt_node) else {
            return false;
        };
        if export.is_default_export {
            return true;
        }
        if export.module_specifier.is_none() {
            return false;
        }
        if export.export_clause.is_none() {
            return true;
        }
        if arena
            .get_identifier_text(export.export_clause)
            .is_some_and(|name| name == "default")
        {
            return true;
        }

        let Some(clause_node) = arena.get(export.export_clause) else {
            return false;
        };
        if clause_node.kind == SyntaxKind::Identifier as u16
            || clause_node.kind == SyntaxKind::StringLiteral as u16
        {
            return true;
        }
        if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
            return false;
        }
        let Some(named) = arena.get_named_imports(clause_node) else {
            return false;
        };
        named.elements.nodes.iter().any(|&spec_idx| {
            let Some(spec) = arena.get_specifier_at(spec_idx) else {
                return false;
            };
            let export_ident = if spec.name.is_some() {
                spec.name
            } else {
                spec.property_name
            };
            arena
                .get_identifier_text(export_ident)
                .is_some_and(|name| name == "default")
        })
    })
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
            Arc::new(Vec::new()),
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
        lib_files: Arc<Vec<Arc<LibFile>>>,
    ) -> Self {
        let content_hash = hash_source_content(&source_text);
        let mut parser = ParserState::new(file_name.clone(), source_text);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        if file_idx != u32::MAX {
            binder.set_file_idx(file_idx);
        }
        bind_with_optional_libs(&mut binder, arena, root, &lib_files);

        let line_map = LineMap::build(parser.get_source_text());
        let export_signature = ExportSignature::compute(&binder, &file_name);
        let has_wildcard_reexport = compute_has_wildcard_reexport(arena, root);

        Self {
            file_name,
            root,
            parser,
            binder,
            line_map,
            has_wildcard_reexport,
            type_interner,
            definition_store: None,
            type_cache: None,
            scope_cache: ScopeCache::default(),
            strict,
            lib_files,
            diagnostics_dirty: false,
            cached_diagnostics: None,
            diagnostics_result_id: None,
            diagnostics_generation: 0,
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
    pub(super) fn with_full_project_context(
        file_name: String,
        source_text: String,
        strict: bool,
        type_interner: Arc<TypeInterner>,
        definition_store: Arc<DefinitionStore>,
        file_idx: u32,
        lib_files: Arc<Vec<Arc<LibFile>>>,
    ) -> Self {
        let mut file = Self::with_shared_interner_and_file_idx(
            file_name,
            source_text,
            strict,
            type_interner,
            file_idx,
            lib_files,
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
    pub fn provider_context(&self) -> LspProviderContext<'_> {
        LspProviderContext {
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

    /// Build the checker-facing lib contexts from this file's lib files.
    ///
    /// Each context is a cheap `Arc` clone of a pre-parsed, pre-bound lib file's
    /// arena and binder. Returns an empty `Vec` in standalone mode (no libs),
    /// in which case providers fall back to their no-lib construction.
    fn lib_contexts(&self) -> Vec<LibContext> {
        LibContext::from_lib_files(&self.lib_files)
    }

    /// Build the full provider options for the type-aware providers (hover,
    /// completions, signature help) from this file's strict flag and the given
    /// lib contexts.
    ///
    /// An empty `lib_contexts` slice yields options equivalent to the historical
    /// `with_strict` construction, so all three providers can use the single
    /// `with_options_and_lib_contexts` constructor unconditionally.
    fn provider_options<'a>(
        &self,
        lib_contexts: &'a [LibContext],
    ) -> crate::provider_macro::FullProviderOptions<'a> {
        crate::provider_macro::FullProviderOptions {
            strict: self.strict,
            sound_mode: false,
            checker_options: Some(checker_options_for(self.strict)),
            lib_contexts,
        }
    }

    /// Replace this file's lib files and re-bind so the new global symbols are
    /// merged into the binder.
    ///
    /// Called by `Project::set_lib_files` when libs are installed (or change)
    /// after a file has already been loaded. Re-parses and re-binds from the
    /// current source text; per-file analysis caches are dropped by the
    /// re-bind so subsequent diagnostics/hover/completions observe the libs.
    pub(crate) fn reload_with_lib_files(&mut self, lib_files: Arc<Vec<Arc<LibFile>>>) {
        self.lib_files = lib_files;
        let current_source = self.parser.get_source_text().to_string();
        self.update_source(current_source);
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
            if let Some(s) = sym.import_module() {
                size += s.len();
            }
            if let Some(s) = sym.import_name() {
                size += s.len();
            }
        }

        // file_locals
        size += b.file_locals.len() * (32 + std::mem::size_of::<SymbolId>());

        // declared_modules
        for s in b.declared_modules.iter() {
            size += s.capacity() + 8;
        }

        // node_symbols
        size += b.node_symbols.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<SymbolId>() + 8);

        // scopes
        size += b.scopes.capacity() * std::mem::size_of::<tsz_binder::Scope>();
        for scope in b.scopes.iter() {
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

        // LSP scope cache: per-file retained scope-chain snapshots.
        size += scope_cache_estimated_size_bytes(&self.scope_cache);

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
        bind_with_optional_libs(&mut self.binder, arena, self.root, &self.lib_files);

        self.line_map = LineMap::build(self.parser.get_source_text());
        let has_wildcard_reexport = compute_has_wildcard_reexport(arena, self.root);
        self.reset_analysis_state();
        self.export_signature = ExportSignature::compute(&self.binder, &self.file_name);
        self.has_wildcard_reexport = has_wildcard_reexport;
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
        // The incremental binder does not merge lib symbols, so when lib files
        // are present we force a full lib-aware rebind rather than risk losing
        // the merged global scope. The `||` short-circuits before the
        // incremental attempt so the file is bound exactly once.
        if !self.lib_files.is_empty()
            || !self.binder.bind_source_file_incremental(
                arena,
                self.root,
                &plan.prefix_nodes,
                &old_suffix_nodes,
                &parse_result.statements.nodes,
                plan.reparse_start,
            )
        {
            self.binder.reset();
            if self.file_idx != u32::MAX {
                self.binder.set_file_idx(self.file_idx);
            }
            bind_with_optional_libs(&mut self.binder, arena, self.root, &self.lib_files);
        }
        let has_wildcard_reexport = compute_has_wildcard_reexport(arena, self.root);
        self.reset_analysis_state();
        self.export_signature = ExportSignature::compute(&self.binder, &self.file_name);
        self.has_wildcard_reexport = has_wildcard_reexport;

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
        self.cached_diagnostics = None;
        self.diagnostics_result_id = None;
    }

    pub fn get_hover(&mut self, position: Position) -> Option<HoverInfo> {
        self.get_hover_with_stats(position, None)
    }

    pub fn get_hover_with_stats(
        &mut self,
        position: Position,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        let lib_contexts = self.lib_contexts();
        let provider = HoverProvider::with_options_and_lib_contexts(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.provider_options(&lib_contexts),
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
        let lib_contexts = self.lib_contexts();
        let provider = SignatureHelpProvider::with_options_and_lib_contexts(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.provider_options(&lib_contexts),
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
        let lib_contexts = self.lib_contexts();
        let provider = Completions::with_options_and_lib_contexts(
            self.parser.get_arena(),
            &self.binder,
            &self.line_map,
            &self.type_interner,
            self.parser.get_source_text(),
            self.file_name.clone(),
            self.provider_options(&lib_contexts),
        );

        provider.get_completions_with_caches(
            self.root,
            position,
            &mut self.type_cache,
            &mut self.scope_cache,
            scope_stats,
        )
    }

    /// Diagnostics for this file, served from `cached_diagnostics` when no
    /// invalidation happened since the last recompute.
    ///
    /// Standalone (non-`Project`) entry point: every mutation path of a
    /// standalone file (`update_source`, `update_source_with_edits`,
    /// `invalidate_caches`) goes through `reset_analysis_state`, which clears
    /// the cache. Files owned by a `Project` are served through the
    /// generation-checked `Project` wrapper instead, which adds the coarse
    /// cross-file invalidation barrier on top of this flag.
    pub fn get_diagnostics(&mut self) -> Vec<LspDiagnostic> {
        if !self.diagnostics_dirty
            && let Some(cached) = &self.cached_diagnostics
        {
            return cached.clone();
        }
        self.compute_diagnostics()
    }

    /// Run the checker over this file unconditionally, refresh
    /// `cached_diagnostics`, and clear `diagnostics_dirty`.
    pub(crate) fn compute_diagnostics(&mut self) -> Vec<LspDiagnostic> {
        let file_name = self.file_name.clone();
        let source_text = self.parser.get_source_text();
        let compiler_options = checker_options_for(self.strict);

        let query_cache = tsz_solver::construction::QueryCache::new(&self.type_interner);

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

        // Seed the standard-library contexts so the checker can resolve global
        // values/types (`Date`, `Map`, `Math`, ...) instead of emitting spurious
        // TS2304/TS2552/TS2583. No-op in standalone mode (empty lib files).
        let lib_contexts = self.lib_contexts();
        if !lib_contexts.is_empty() {
            checker.ctx.set_lib_contexts(lib_contexts);
        }

        checker.check_source_file(self.root);

        let diagnostics: Vec<LspDiagnostic> = checker
            .ctx
            .diagnostics
            .iter()
            .map(|diag| convert_diagnostic(diag, &self.line_map, source_text))
            .collect();

        self.type_cache = Some(checker.extract_cache());
        self.diagnostics_dirty = false;
        self.cached_diagnostics = Some(diagnostics.clone());
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
        if symbol.is_exported {
            names.push(local_name.to_string());
        }

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
