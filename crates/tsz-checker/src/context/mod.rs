//! Checker Context
//!
//! Holds the shared state used throughout the type checking process.
//! This separates state from logic, allowing specialized checkers (expressions, statements)
//! to borrow the context mutably.
mod aliases;
mod analysis_state_types;
mod cache_statistics;
mod caches;
mod canonical_app_key;
mod compiler_options;
mod cross_file_delegation_cache;
mod cross_file_type_params_cache;
pub use cache_statistics::CheckerContextCacheStatistics;
pub use caches::{
    AssignabilityEvalMemo, AssignabilityEvalStamp, AssignabilityFailureKey,
    AssignabilityFailureMemo, AwaitedAssignabilityEvalMemo, CachedAssignabilityAnalysis, CowCache,
    GenericConstraintProofKey, GenericConstraintProofMemo, NarrowableIdentifierCache,
    NodeTypeCache, SharedConstraintProofCache, SymbolTypeCache, TypeNodeSurfaceCaches,
    TypeReferenceValidationCaches,
};
pub use canonical_app_key::CanonicalAppKey;
pub(crate) use compiler_options::is_declaration_file_name;
pub(crate) use compiler_options::is_js_file_name;
pub(crate) use compiler_options::should_resolve_jsdoc_for_file;
pub use cross_file_delegation_cache::{CrossArenaSessionMemo, CrossFileDelegationCache};
pub use cross_file_type_params_cache::{
    CrossFileTypeParamsCacheStatistics, cross_file_type_params_cache_statistics,
};
pub(crate) mod constructors;
mod core;
mod core_module_resolution;
pub use core::build_lib_file_local_names;
mod cross_file_query;
mod diagnostic_indices;
mod diagnostic_push;
pub(crate) mod env_eval_cache;
/// Debug-only purity invariant for the stamp-keyed evaluation memos (#13980).
/// Compiles out of release builds; the wiring in `caches.rs` is gated the same.
#[cfg(debug_assertions)]
pub(crate) mod eval_memo_purity;
mod file_session_reset;
mod generator_yield_contribution;
pub use generator_yield_contribution::GeneratorYieldContribution;
pub mod lifetime_shells;
pub use lifetime_shells::{FileSession, LspPersistentCache, SpeculationScope, WorkerContext};
mod def_declaration;
mod def_mapping;
mod def_mapping_env_writes;
mod def_mapping_flow_mirror;
mod def_mapping_formatters;
mod def_mapping_resolved_env;
mod deferred_flow_env_write;
mod unresolved_import;
pub use deferred_flow_env_write::DeferredFlowEnvWrite;
mod file_format_lookup;
pub(crate) use file_format_lookup::{lookup_file_is_esm_in_map, lookup_is_external_module_in_map};
mod import_alias_resolution;
mod import_conflicts;
mod module_format;
mod parse_health;
pub use parse_health::ParseHealth;
mod import_assert_file_scan;
mod import_extension_flags;
mod lib_type_resolution_caches;
pub use lib_type_resolution_caches::LibTypeResolutionCaches;
mod lib_queries;
mod module_entity;
mod package_resolution;
mod program_context;
pub use program_context::ProgramContext;
mod request_cache;
mod resolver;
mod source_file_symbol_type_cache_scope;
pub(crate) mod speculation;
mod strict_mode;
mod symbol_file_targets;
mod type_parameter_scope_cache_key;
pub mod typing_request;
use crate::control_flow::FlowGraph;
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::{QueryDatabase, TypeEnvironment};
pub use aliases::*;
pub use analysis_state_types::*;
pub(crate) use diagnostic_indices::DiagnosticIndices;
pub use request_cache::{RequestCacheCounters, RequestCacheKey};
use rustc_hash::{FxHashMap, FxHashSet};
use source_file_symbol_type_cache_scope::next_source_file_symbol_type_cache_scope;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
pub use symbol_file_targets::SymbolFileTargetsOverlay;
use tsz_binder::{ScopeId, SymbolId};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;
use tsz_solver::def::{DefId, DefinitionStore};
pub use type_parameter_scope_cache_key::TypeParameterScopeCacheKey;
pub use typing_request::{ContextualOrigin, FlowIntent, TypingRequest};
// Re-export context-facing types used by downstream crates.
pub use tsz_binder::LibContext;
use tsz_binder::{BinderState, ModuleAugmentation};
pub use tsz_common::checker_options::CheckerOptions;
pub use tsz_common::common::ScriptTarget;
use tsz_parser::parser::node::NodeArena;

/// T2.2 cross-file type-parameter memoization map.
///
/// Keyed by `(target_file_idx, decl_idx)` — never by user-chosen
/// identifier names — and stores the `Vec<TypeParamInfo>` produced by
/// the slow path. Shared across every checker via `Arc` so the second
/// caller sees the first caller's work.
pub type CrossFileTypeParamsCache =
    Arc<dashmap::DashMap<(u32, NodeIndex), Vec<tsz_solver::TypeParamInfo>>>;

/// Overflow state observed from relation checks that feed assignability
/// diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationOverflowFlags {
    pub depth_exceeded: bool,
    pub iteration_exceeded: bool,
}

impl RelationOverflowFlags {
    pub const fn has_overflow(self) -> bool {
        self.depth_exceeded || self.iteration_exceeded
    }

    pub const fn merge(&mut self, depth_exceeded: bool, iteration_exceeded: bool) {
        self.depth_exceeded |= depth_exceeded;
        self.iteration_exceeded |= iteration_exceeded;
    }
}

/// Project-run cache for global JSDoc typedef lookup.
pub struct JSDocGlobalTypedefLookupCache {
    pub miss_cache: RefCell<FxHashSet<String>>,
    pub in_progress: RefCell<FxHashSet<String>>,
    pub typedef_presence_by_file: Arc<dashmap::DashMap<(u32, u32, String), bool>>,
    /// Name-independent per-file guard: whether a source file contains any real
    /// JSDoc `@typedef` / `@callback` / `@import` *tag* (line-start in a JSDoc
    /// comment), as opposed to a coincidental substring in doc prose (the DOM
    /// lib mentions the CSS `@import` at-rule dozens of times). Computed once
    /// per file and reused for every unresolved-name lookup, so a typedef-free
    /// file (or lib) is scanned+parsed at most once instead of once per name.
    pub tag_presence_by_file: Arc<dashmap::DashMap<(u32, u32), bool>>,
}

/// Maximum depth for nested `get_type_of_symbol` calls before giving up.
///
/// Prevents stack overflow when resolving deeply recursive or circular
/// symbol references (e.g., mutually referencing type aliases, deeply
/// nested namespace exports). Matches `MAX_INSTANTIATION_DEPTH` (50).
pub(crate) const MAX_SYMBOL_RESOLUTION_DEPTH: u32 = 50;

mod global_declared_modules;
pub use global_declared_modules::{GlobalDeclaredModules, ambient_pattern_matches};

/// Info about the enclosing class for static member suggestions and abstract property checks.
#[derive(Clone, Debug)]
pub struct EnclosingClassInfo {
    /// Name of the class.
    pub name: String,
    /// Node index for the class declaration.
    pub class_idx: NodeIndex,
    /// Member node indices for symbol lookup.
    pub member_nodes: Vec<NodeIndex>,
    /// Whether we're in a constructor (for error 2715 checking).
    pub in_constructor: bool,
    /// Whether this is a `declare class` (ambient context for error 1183).
    pub is_declared: bool,
    /// Whether we're in a static property initializer (for TS17011 checking).
    pub in_static_property_initializer: bool,
    /// Whether we're in a static method or property context.
    pub in_static_member: bool,
    /// Whether any `super()` call appeared while checking the current constructor body.
    pub has_super_call_in_current_constructor: bool,
    /// Cached instance `this` type for members of this class.
    pub cached_instance_this_type: Option<TypeId>,
    /// Names of the class's own type parameters (for TS2302 checking in static members).
    pub type_param_names: Vec<String>,
    /// The type parameter infos of the class's own type parameters.
    pub class_type_parameters: Vec<tsz_solver::TypeParamInfo>,
    /// Exact interned identities of the class's own type parameters, in
    /// declaration order. Kept separately from the name-keyed active scope so
    /// method-level shadowing cannot hide the enclosing class binders.
    pub class_type_parameter_ids: Vec<TypeId>,
    /// `async_depth` as it was immediately before member checking resets it
    /// to `0` for the class body (field initializers and static blocks don't
    /// inherit `async` from the enclosing function). A member's *computed
    /// name* is evaluated once, when the class itself is defined, in this
    /// enclosing scope — not inside that reset — so `await` grammar checks on
    /// a computed name must consult this value instead of the live
    /// `async_depth`.
    pub enclosing_async_depth: u32,
}

/// Info about a label in scope for break/continue validation.
#[derive(Clone, Debug)]
pub struct LabelInfo {
    /// The label name (e.g., "outer").
    pub(crate) name: String,
    /// Whether the label is on an iteration statement (for continue validation).
    /// Only iteration labels can be targets of continue statements.
    pub(crate) is_iteration: bool,
    /// The function depth when this label was defined.
    /// Used to detect if a jump crosses a function boundary.
    pub(crate) function_depth: u32,
    /// Whether the label was targeted by a break/continue statement.
    /// Used for TS7028 (unused label) detection.
    pub(crate) referenced: bool,
    /// The AST node index of the label identifier (for error reporting).
    pub(crate) label_node: tsz_parser::parser::NodeIndex,
}

/// The loop/`switch`/label state of the function-like a nested body is being
/// checked inside of, saved by
/// [`CheckerContext::enter_function_like_control_flow`].
///
/// Opaque on purpose: the fields are only ever produced and consumed by that
/// enter/exit pair, so no caller can restore a partial scope.
#[derive(Clone, Copy, Debug)]
pub struct FunctionLikeControlFlow {
    iteration_depth: u32,
    switch_depth: u32,
    label_stack_len: usize,
    had_outer_loop: bool,
}

/// The scope of a class member body, saved by
/// [`CheckerContext::enter_class_member_body`].
///
/// Bundles the member-body baseline with the [`FunctionLikeControlFlow`] scope
/// so that every member kind — method, accessor, constructor, static block —
/// crosses the boundary through one call and cannot acquire the function-depth
/// half without the control-flow half.
#[derive(Clone, Copy, Debug)]
pub struct ClassMemberBodyScope {
    member_body_depth: u32,
    control_flow: FunctionLikeControlFlow,
}

/// Classification for deferred implicit-any diagnostics that are surfaced later
/// at use sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingImplicitAnyKind {
    /// Bare implicit-any variables only become errors when captured by a nested
    /// function boundary.
    CaptureOnly,
    /// Evolving `any[]` variables become errors at unsafe reads before their
    /// element type is fixed.
    EvolvingArray,
}

/// Persistent cache for type checking results across LSP queries.
/// This cache survives between LSP requests but is invalidated when the file changes.
#[derive(Clone, Debug, Default)]
pub struct TypeCache {
    /// Cached types for symbols (dense flat-vec, O(1) lookup by symbol index).
    pub symbol_types: SymbolTypeCache,

    /// Cached instance types for class symbols (dense flat-vec, O(1) lookup by symbol index).
    /// Distinguishes from `symbol_types` which holds constructor types for VALUE position.
    pub symbol_instance_types: SymbolTypeCache,

    /// Cached types for nodes (dense flat-vec, O(1) lookup by node index).
    pub node_types: NodeTypeCache,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Maps `DefIds` to `SymbolIds` for declaration emit usage analysis.
    /// Populated by `CheckerContext` during type checking, consumed by `UsageAnalyzer`.
    pub def_to_symbol: FxHashMap<tsz_solver::DefId, SymbolId>,

    /// Maps `DefIds` to symbol name strings for declaration emit.
    pub def_to_name: FxHashMap<tsz_solver::DefId, String>,

    /// Snapshot of resolved `DefId -> TypeId` bodies for declaration emit evaluation.
    pub def_types: FxHashMap<u32, TypeId>,

    /// Snapshot of resolved `DefId -> type params` for declaration emit evaluation.
    pub def_type_params: FxHashMap<u32, Vec<tsz_solver::TypeParamInfo>>,

    pub boxed_types: FxHashMap<tsz_solver::IntrinsicKind, TypeId>,
    pub boxed_def_ids: FxHashMap<tsz_solver::IntrinsicKind, Vec<tsz_solver::DefId>>,
    pub well_known_symbol_names: FxHashMap<String, tsz_solver::SymbolRef>,
    /// Cache for control flow analysis results.
    /// Key: (`FlowNodeId`, `SymbolId`, `InitialTypeId`) -> `NarrowedTypeId`
    pub flow_analysis_cache:
        FxHashMap<(tsz_binder::FlowNodeId, tsz_binder::SymbolId, TypeId), TypeId>,

    /// Maps class instance `TypeIds` to their class declaration `NodeIndex`.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration `NodeIndex` -> computed instance `TypeId`.
    /// Avoids recomputing the full class instance type on every member check.
    ///
    /// `RefCell` for `&self` interior mutability (#12101); an on-demand `&self`
    /// forcing resolver can publish without a `&mut` borrow. Behavior identical.
    pub class_instance_type_cache: RefCell<FxHashMap<NodeIndex, TypeId>>,

    /// Forward cache: class declaration `NodeIndex` -> computed constructor `TypeId`.
    /// Avoids recomputing constructor shape/inheritance on repeated class queries.
    ///
    /// Wrapped in `RefCell` for `&self` interior mutability (#12101); see
    /// `class_instance_type_cache`.
    pub class_constructor_type_cache: RefCell<FxHashMap<NodeIndex, TypeId>>,

    /// Set of import specifier nodes that should be elided from JavaScript output.
    /// These are imports that reference type-only declarations (interfaces, type aliases).
    pub type_only_nodes: FxHashSet<NodeIndex>,

    /// Maps namespace `TypeIds` to their module display name.
    /// Used to display namespace types as `typeof import("module")` in diagnostics.
    /// Persists across file checks because `NS_CONSTRUCT` may run for one file
    /// while TS2339 is emitted from another.
    pub namespace_module_names: FxHashMap<TypeId, String>,
}

impl TypeCache {
    /// Estimate the resident heap bytes of this per-file cache snapshot.
    ///
    /// Entry-count-based estimate for residency accounting (#13249 step 1):
    /// emit runs pin one `TypeCache` per file until emit completes, so the
    /// CLI driver sums this across the retained map at perf-counter
    /// snapshot time. Never called on a checking hot path.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        // FxHashMap per-entry overhead: hash + bucket slot + padding.
        const ENTRY_OVERHEAD: usize = 16;
        let entry = |len: usize, key: usize, value: usize| len * (ENTRY_OVERHEAD + key + value);
        let id = std::mem::size_of::<TypeId>();
        let node = std::mem::size_of::<NodeIndex>();
        let sym = std::mem::size_of::<SymbolId>();

        let mut size = std::mem::size_of::<Self>();
        size += entry(self.symbol_types.len(), sym, id);
        size += entry(self.symbol_instance_types.len(), sym, id);
        size += entry(self.node_types.len(), 4, id);
        size += entry(
            self.symbol_dependencies.len(),
            sym,
            std::mem::size_of::<FxHashSet<SymbolId>>(),
        );
        size += self
            .symbol_dependencies
            .values()
            .map(|deps| deps.len() * (ENTRY_OVERHEAD + sym))
            .sum::<usize>();
        size += entry(self.def_to_symbol.len(), 8, sym);
        size += entry(self.def_to_name.len(), 8, std::mem::size_of::<String>());
        size += self
            .def_to_name
            .values()
            .map(String::capacity)
            .sum::<usize>();
        size += entry(self.def_types.len(), 4, id);
        size += entry(
            self.def_type_params.len(),
            4,
            std::mem::size_of::<Vec<tsz_solver::TypeParamInfo>>(),
        );
        size += self
            .def_type_params
            .values()
            .map(|params| params.capacity() * std::mem::size_of::<tsz_solver::TypeParamInfo>())
            .sum::<usize>();
        size += entry(self.boxed_types.len(), 8, id);
        size += entry(
            self.boxed_def_ids.len(),
            8,
            std::mem::size_of::<Vec<tsz_solver::DefId>>(),
        );
        size += entry(
            self.well_known_symbol_names.len(),
            std::mem::size_of::<String>(),
            std::mem::size_of::<tsz_solver::SymbolRef>(),
        );
        size += entry(self.flow_analysis_cache.len(), 8 + sym + id, id);
        size += entry(self.class_instance_type_to_decl.len(), id, node);
        size += entry(self.class_instance_type_cache.borrow().len(), node, id);
        size += entry(self.class_constructor_type_cache.borrow().len(), node, id);
        size += entry(self.type_only_nodes.len(), node, 0);
        size += entry(
            self.namespace_module_names.len(),
            id,
            std::mem::size_of::<String>(),
        );
        size += self
            .namespace_module_names
            .values()
            .map(String::capacity)
            .sum::<usize>();
        size
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvEvalCacheEntry {
    pub(crate) result: TypeId,
    pub(crate) depth_exceeded: bool,
}

/// Info about a symbol that came from destructuring a union type.
/// Used for correlated discriminant narrowing: when `const { data, isSuccess } = getResult()`,
/// narrowing `isSuccess` should also narrow `data`.
#[derive(Clone, Debug)]
pub struct DestructuredBindingInfo {
    /// The source type of the entire destructured expression (the union)
    pub(crate) source_type: TypeId,
    /// The property name that this symbol corresponds to (for object patterns)
    pub(crate) property_name: String,
    /// The element index for array/tuple patterns (`u32::MAX` if object pattern)
    pub(crate) element_index: u32,
    /// The binding group ID — all symbols from the same destructuring share this
    pub(crate) group_id: u32,
    /// Whether this is a const binding (only const bindings support correlated narrowing)
    pub(crate) is_const: bool,
    /// Whether this binding is a rest element (`...rest` in array destructuring)
    pub(crate) is_rest: bool,
}

/// Visible spelling candidates for one file session.
pub type SpellingCandidateCache = FxHashMap<(ScopeId, u32), Rc<[String]>>;

/// Per-spelling suggestion results for one file session.
pub type SpellingSuggestionScanCache = FxHashMap<(ScopeId, u32), FxHashMap<String, Vec<String>>>;

/// Name-resolution diagnostic caches that must stay coordinated.
#[derive(Debug, Default)]
pub struct NameResolutionDiagnostics {
    /// Visible spelling candidates keyed by lexical scope and symbol-meaning
    /// mask. Binder/lib tables are immutable during a file session, so every
    /// miss in the same scope can reuse one candidate universe.
    pub spelling_candidate_cache: RefCell<SpellingCandidateCache>,

    /// Memoized spelling-suggestion candidate scans keyed by lexical scope,
    /// misspelled name, and symbol-meaning mask (`VALUE`/`TYPE`/`NAMESPACE`). The
    /// full-symbol-universe Levenshtein walk depends only on immutable binder/lib
    /// tables and that key. This collapses both demand-driven revisits and repeated
    /// misses in one scope without leaking candidates across scopes.
    /// Cleared at the file-session boundary because `ScopeId` is binder-local.
    pub suggestion_scan_cache: RefCell<SpellingSuggestionScanCache>,
}

/// File-session memo tables shared by flow narrowing helpers.
///
/// Grouping these symbol-stable caches keeps the top-level `CheckerContext`
/// field count bounded as flow memoization grows.
#[derive(Debug, Default)]
pub struct SymbolFlowMemoCaches {
    pub last_assignment_pos: RefCell<FxHashMap<SymbolId, u32>>,
    pub nested_closure_assignment: RefCell<FxHashMap<SymbolId, bool>>,
    pub has_non_initializer_assignment: RefCell<FxHashMap<SymbolId, bool>>,
    pub first_identifier_ref: RefCell<FxHashMap<SymbolId, Option<NodeIndex>>>,
    pub alias_base_assignment: RefCell<FxHashMap<(u32, u32), bool>>,
    pub alias_path_assignment: RefCell<FxHashMap<(u32, u32, u32), bool>>,
}

impl SymbolFlowMemoCaches {
    pub fn clear(&self) {
        self.last_assignment_pos.borrow_mut().clear();
        self.nested_closure_assignment.borrow_mut().clear();
        self.has_non_initializer_assignment.borrow_mut().clear();
        self.first_identifier_ref.borrow_mut().clear();
        self.alias_base_assignment.borrow_mut().clear();
        self.alias_path_assignment.borrow_mut().clear();
    }
}

mod flow_shared;
pub use flow_shared::FlowSharedCaches;

/// Shared state for type checking.
pub struct CheckerContext<'a> {
    /// The `NodeArena` containing the AST.
    pub arena: &'a NodeArena,

    /// The binder state with symbols.
    pub binder: &'a BinderState,

    /// Query database for type interning and memoized type operations.
    /// Supports both `TypeInterner` (via trait upcasting) and `QueryCache`.
    pub types: &'a dyn QueryDatabase,
    /// Current file name.
    pub file_name: String,

    /// Explicit-file cwd for TS6053/TS6054 reference paths; `None` keeps
    /// resolved project/config paths.
    pub current_directory: Option<Arc<str>>,

    /// Compiler options for type checking.
    pub compiler_options: CheckerOptions,

    /// Precomputed environment capabilities matrix.
    /// Centralizes lib/config/feature-gate queries for diagnostic routing.
    pub capabilities: crate::query_boundaries::capabilities::EnvironmentCapabilities,

    /// Whether `noImplicitOverride` diagnostics are enabled for this source file.
    pub no_implicit_override: bool,

    /// Whether unresolved import diagnostics should be emitted by the checker.
    /// The CLI driver handles module resolution in multi-file mode.
    ///
    /// Checker invariant: when driver-provided resolution context is available,
    /// checker should consume that context and avoid ad-hoc module-existence inference.
    pub report_unresolved_imports: bool,

    /// Whether leading conformance-style source comments may override compiler options.
    ///
    /// TypeScript source files can contain comments like `// @strict: false` in
    /// the conformance suite, but those are not user-facing source directives.
    /// Normal CLI/LSP/project checking must leave compiler options unchanged.
    pub allow_source_file_test_pragmas: bool,

    /// Whether the current file is an ESM module (per-file determination).
    /// In Node16/NodeNext, `.js`/`.ts` files may be ESM based on the nearest
    /// `package.json` `"type": "module"` field. Set by the driver from module resolver.
    /// When `None`, the checker falls back to extension + global module kind heuristic.
    pub file_is_esm: Option<bool>,

    /// Per-file ESM/CJS map for ALL project files, keyed by file path.
    /// Used by TS1479 to determine if an import *target* is ESM (not just the current file).
    /// Set by the driver from module resolver; `None` in single-file / non-Node modes.
    pub file_is_esm_map: Option<Arc<FxHashMap<String, bool>>>,

    /// Tracking the current computed property name node for TS2467
    pub checking_computed_property_name: Option<NodeIndex>,

    /// Name-resolution diagnostic scan cache.
    pub name_resolution_diagnostics: NameResolutionDiagnostics,

    /// `TypeId`s that represent interfaces extending arrays/tuples.
    /// Used to suppress false TS2559 (weak type) violations for these types,
    /// since they inherit non-optional members from Array.prototype.
    pub types_extending_array: FxHashSet<TypeId>,

    /// Recovery sites; see `crate::recovery`.
    pub(crate) recovery_sites: RefCell<crate::recovery::RecoverySites>,

    // --- Caches ---
    /// Cached types for symbols (dense flat-vec, O(1) lookup by symbol index).
    pub symbol_types: SymbolTypeCache,

    /// Cached instance types for class symbols (dense flat-vec, O(1) lookup by symbol index).
    /// Distinguishes from `symbol_types` which holds constructor types for VALUE position.
    pub symbol_instance_types: SymbolTypeCache,

    /// Cached namespace object types for enums (for `typeof Enum` / `keyof typeof Enum`).
    /// Maps enum `SymbolId` → namespace object `TypeId` with member names as properties.
    pub enum_namespace_types: CowCache<FxHashMap<SymbolId, TypeId>>,

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

    /// File-session caches for lib type and lazy lib-member resolution.
    pub lib_type_resolution_caches: LibTypeResolutionCaches,

    /// File-local caches for cross-file/lib delegation results.
    pub lib_delegation_cache: CrossFileDelegationCache,

    /// Per-checker cache for cross-binder namespace member resolution.
    /// Keyed by (`namespace_name`, `member_name`) and stores both hits and misses.
    /// This avoids repeatedly rescanning all binders for hot qualified React lookups
    /// like `React.Component`, `React.ComponentClass`, `React.ReactNode`, etc.
    pub namespace_member_resolution_cache: RefCell<CowCache<NamespaceMemberResolutionCache>>,

    /// Per-checker cache for named exports resolved through `export=`.
    /// Misses are cached only for lookups that enter without alias-cycle state.
    pub export_equals_named_cache: RefCell<CowCache<ExportEqualsNamedCache>>,

    /// Per-checker cache for nested namespace candidates found through namespace exports.
    /// Keyed by `namespace_name` and stores the candidate nested namespace symbols with
    /// their owning file index. This avoids rescanning every binder when resolving many
    /// different members from the same nested namespace.
    pub nested_namespace_candidates_cache: RefCell<CowCache<NestedNamespaceCandidatesCache>>,

    /// Per-checker cache for same-name symbol candidates across the current binder
    /// and all cross-file binders.
    pub symbol_name_candidates_cache: RefCell<FxHashMap<String, Vec<SymbolId>>>,

    /// Per-file memo for `find_member_access_info`, keyed by
    /// `(class declaration NodeIndex, member-name Atom, is_static)`. The lookup
    /// walks the class's `members.nodes` (and its base-class chain) by name to
    /// classify a member's access level, which is `O(members)` per access; with
    /// one access per method on an `N`-member class that linear scan turns into
    /// `O(N^2)`. The result is a pure function of the immutable AST for the
    /// current file, so a `(node, name, static)`-keyed memo collapses the
    /// repeated scans to one walk per distinct member request.
    ///
    /// Keyed on file-local `NodeIndex`, so it is cleared at the file-session
    /// reset boundary alongside the other node-keyed caches.
    pub(crate) member_access_info_cache: MemberAccessInfoCache,

    /// Per-file memo for "does the enclosing class directly declare a member
    /// named X", keyed by `(class declaration NodeIndex, member-name Atom)`. The
    /// predicate scans the enclosing class's `member_nodes` by name, which is
    /// `O(members)` per `this.x` access; one access per method on an `N`-member
    /// class makes it `O(N^2)`. The answer is a pure function of the immutable
    /// AST for the current file. Cleared at the file-session reset boundary
    /// because the `NodeIndex` key is file-local.
    pub(crate) enclosing_class_declares_member_cache: RefCell<FxHashMap<(NodeIndex, Atom), bool>>,

    /// Per-file memo for `find_accessor_levels_in_hierarchy`, keyed by
    /// `(class declaration NodeIndex, member-name Atom, is_static)`. The lookup
    /// scans the class's `members.nodes` for a getter/setter pair (walking the
    /// base-class chain) per property access — `O(members)` each, so one access
    /// per method on an `N`-member class makes it `O(N^2)`. The accessor levels
    /// are a pure function of the immutable AST for the current file. Cleared at
    /// the file-session reset boundary because the `NodeIndex` keys are
    /// file-local.
    pub(crate) accessor_levels_cache: AccessorLevelsCache,

    /// Negative results and re-entrancy state for global JSDoc typedef lookup.
    /// Misses are stable for the project run; the in-progress set prevents a
    /// recursive typedef's cycle-break `None` from poisoning that miss cache.
    pub jsdoc_global_typedef_lookup_cache: JSDocGlobalTypedefLookupCache,

    /// True once `nested_namespace_candidates_cache` has been populated for every
    /// nested namespace export name visible across all binders.
    pub nested_namespace_candidates_cache_complete: Cell<bool>,

    /// Per-checker cache for text-based entity-name resolution used by lowering.
    /// Keyed by names like `React.ReactNode` / `JSX.Element` and stores both
    /// hits and misses to avoid repeatedly walking the same symbol graph during
    /// declaration-file interface/type lowering.
    pub lowering_entity_name_resolution_cache: RefCell<CowCache<FxHashMap<String, Option<DefId>>>>,

    /// Per-checker cache for cross-file namespace export resolution.
    /// Keyed by the requesting file and module specifier because relative
    /// specifiers are resolved from the current file.
    pub namespace_exports_cache: RefCell<NamespaceExportsCache>,

    /// Per-checker memo for re-export chain resolution, keyed by
    /// `(target file, export name)`. Populated only by root resolutions of
    /// [`crate::state::CheckerState::resolve_export_in_file`] (entered with an
    /// empty `visited` path), whose answer is the canonical, path-independent
    /// result for that pair. Collapses the `O(names × export-edges)` re-walk of
    /// barrel `export *` graphs to a single walk per distinct `(file, name)`.
    pub reexport_resolution_cache: RefCell<ReexportResolutionCache>,

    /// Shared lib type resolution cache across parallel file checks.
    /// Uses `DashMap` for thread-safe concurrent access.
    pub shared_lib_type_cache: Option<Arc<dashmap::DashMap<String, Option<TypeId>>>>,

    /// Program-wide success tier for generic type-argument constraint proofs,
    /// shared across parallel file checks. Consulted on local
    /// `type_reference_validation_caches` misses; only file-independent
    /// successes are published. See
    /// [`crate::context::SharedConstraintProofCache`].
    pub shared_constraint_proofs: Option<Arc<crate::context::SharedConstraintProofCache>>,
    // T2.2 cross-file type-parameter cache type alias is defined just below.
    /// Program-wide memoization for `extract_type_params_from_decl` slow-path
    /// results, keyed by `(target_file_idx, decl_idx)`. T2.2: collapses
    /// redundant child-checker constructions on the `TypeEnvironmentCore`
    /// path (the dominant share of `with_parent_cache_constructed` per
    /// the 2026-05-10 attribution run, ~84 % on the cliff fixtures).
    /// Populated lazily on slow-path completion; consulted before any new
    /// `with_parent_cache_attributed(..., TypeEnvironmentCore)` call.
    pub cross_file_type_params_cache: Option<CrossFileTypeParamsCache>,

    /// When true, `resolve_lib_type_by_name` returns `None` immediately without
    /// resolving lib types. Set when TS5107/TS5101 deprecation diagnostics are
    /// present — tsc stops compilation at TS5107 and never type-checks files.
    /// The checker still walks the AST (finding grammar errors like TS17006),
    /// but all type resolution is short-circuited to avoid the O(n²) memory
    /// explosion from 48+ files independently resolving es5 heritage chains.
    pub skip_lib_type_resolution: bool,

    /// Names currently being resolved in `merge_lib_interface_heritage`.
    /// Used to break cycles in the `resolve_lib_type_by_name` ↔ `merge_lib_interface_heritage`
    /// mutual recursion (e.g., Array extends ReadonlyArray which extends Iterable ...).
    pub lib_heritage_in_progress: CowCache<FxHashSet<String>>,

    /// Cached types for nodes (dense flat-vec, O(1) lookup by node index).
    pub node_types: NodeTypeCache,

    /// Request-aware cache for audited non-empty request paths only.
    pub request_node_types: CowCache<FxHashMap<(u32, RequestCacheKey), TypeId>>,

    /// Object-literal diagnostic recovery and active initializer state.
    pub object_literal_tracking: ObjectLiteralTracking,

    /// Internal counters for request-aware cache usage and cache-clear churn.
    pub request_cache_counters: RequestCacheCounters,

    /// Flow-analyzer `TypeEnvironment` (#13086).
    ///
    /// One of the context's two `TypeEnvironment` instances. This one is owned
    /// by the flow analyzer: `FlowAnalyzer::from_ctx` borrows it via
    /// `with_type_environment(&ctx.type_environment)` and holds that borrow live
    /// while narrowing reads types. It also carries the legacy `SymbolRef`-keyed
    /// entries the evaluator env never uses.
    ///
    /// It is *not* a redundant copy of [`Self::type_env`]: the two are separate
    /// `RefCell`s on purpose. While the flow analyzer holds this env borrowed,
    /// the evaluator must still be able to `try_borrow_mut` [`Self::type_env`] to
    /// publish freshly resolved `DefId -> TypeId` bodies; collapsing both into
    /// one cell would make that mutable borrow fail and silently drop writes.
    /// The dual-write registration helpers (`register_*_in_envs`) mirror every
    /// `DefId`-keyed registration into both envs (deferring the mirror when this
    /// cell is borrowed, never dropping it). After file preparation the two envs
    /// must agree on shared entries and the flow env must not be missing entries
    /// from the evaluator env; debug assertions enforce that boundary.
    pub type_environment: RefCell<TypeEnvironment>,

    /// Dual-env registrations whose mirror-write into `type_environment` lost
    /// the `RefCell` borrow race (the flow-analyzer env was already borrowed
    /// during recursive resolution). Rather than silently dropping the write
    /// and repairing the divergence with a full per-file `clone()`, the missed
    /// operation is reified here and replayed the next time `type_environment`
    /// is borrowable. This eliminates borrow-conflict misses at their source.
    pub deferred_flow_env_writes: RefCell<Vec<DeferredFlowEnvWrite>>,

    /// Dual-env registrations whose authoritative write into `type_env` (the
    /// evaluator env) lost the `RefCell` borrow race because `type_env` was
    /// already borrowed during recursive resolution. Previously such a write was
    /// silently dropped with only a `warn!`, which left the entry absent from
    /// both the local cache and (since the shared-store write-through lives
    /// inside the env mutator) the shared `DefinitionStore` — collapsing a
    /// class-instance / def body to `never` for every later consumer (xstate
    /// `Actor` `this` collapse). The missed operation is reified here and
    /// replayed the next time `type_env` is borrowable, exactly as
    /// `deferred_flow_env_writes` does for the flow-analyzer env.
    pub deferred_eval_env_writes: RefCell<Vec<DeferredFlowEnvWrite>>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

    /// Recursion guard for `evaluate_type_with_resolution`, preventing infinite
    /// mutual recursion (evaluate → prune → impossible-property → evaluate) on
    /// recursive type aliases. Keyed by [`CanonicalAppKey`] (not raw [`TypeId`])
    /// to collapse import-alias variants; see that type for the rationale.
    pub type_resolution_visiting: FxHashSet<CanonicalAppKey>,
    /// Reentrancy guard for `prune_impossible_object_union_members_with_env`.
    /// Prevents infinite mutual recursion: evaluate → prune → evaluate → prune.
    pub pruning_union_members: bool,

    /// Recursion guard for `resolve_jsdoc_typedef_type`.
    /// Prevents infinite recursion when a JSDoc `@typedef` references itself
    /// (e.g., `@typedef {... | Json[]} Json`).
    pub jsdoc_typedef_resolving: RefCell<CowCache<rustc_hash::FxHashSet<String>>>,

    /// Recursion guard for *generic* JSDoc `@typedef` resolution, mapping the
    /// typedef name currently being expanded to the `DefId` of its lazy alias.
    /// A self-recursive generic application such as `@typedef {{ next: Box<T> |
    /// null }} Box` must resolve the inner `Box<T>` to a deferred
    /// `Application(Lazy(DefId), args)` instead of eagerly re-expanding the body,
    /// which would otherwise recurse until the stack overflows. The solver then
    /// resolves the alias coinductively, keyed by `(DefId, type args)`.
    pub jsdoc_generic_typedef_resolving: RefCell<CowCache<rustc_hash::FxHashMap<String, DefId>>>,

    /// File-session shared flow caches, wired as one bundle into every
    /// context-backed `FlowAnalyzer` via `FlowAnalyzer::from_ctx`.
    pub flow_shared: FlowSharedCaches,

    /// Cache for `is_narrowable_identifier` results.
    /// This is pure (depends only on AST structure), so it never needs invalidation.
    /// Avoids 4-5 binder/arena lookups per call on the hot cached-node path.
    /// Uses a dense flat array (1 byte per node) instead of `FxHashMap`.
    pub narrowable_identifier_cache: RefCell<NarrowableIdentifierCache>,

    /// Stable flow cache: maps `(SymbolId, DeclaredTypeId)` to the last `FlowNodeId`
    /// where flow analysis confirmed no narrowing (returned the declared type unchanged).
    /// When a new flow node for the same symbol can reach the confirmed node via a
    /// straight-line chain (no `CONDITION/ASSIGNMENT/BRANCH_LABEL` nodes), flow analysis
    /// is skipped entirely, returning the declared type directly.
    /// This eliminates O(N) flow cache misses for N sequential accesses to the same
    /// identifier (e.g., 34 references to `options` in sequential statements).
    pub symbol_flow_confirmed: RefCell<CowCache<SymbolFlowConfirmedMap>>,

    /// Nodes where TS2454 (used before assigned) was emitted.
    /// When TS2454 fires, `check_flow_usage` returns the declared type (un-narrowed).
    /// The second narrowing pass in `get_type_of_node` must NOT re-narrow these nodes,
    /// otherwise the declared type gets overridden with the narrowed type.
    pub daa_error_nodes: CowCache<FxHashSet<u32>>,

    /// Optional-chain nodes whose computed type's `undefined` member exists
    /// only because the chain can short-circuit — the equivalent of tsc's
    /// optional-type marker (`addOptionalTypeMarker`). Chain continuations
    /// (`o?.f()`, `o?.f.g`, `o?.f["g"]`) remove `undefined` from the receiver
    /// or callee only for nodes recorded here, so a member's own inherent
    /// `undefined`/`null` still drives the possibly-nullish diagnostics
    /// (TS18047/TS18048/TS2721/TS2722). Entries are overwritten whenever the
    /// producing access is recomputed, so stale speculative results
    /// self-correct.
    pub optional_chain_marker_only_nodes: FxHashSet<u32>,

    /// Callback-body expression nodes whose contextual return type was, at some
    /// point during generic call inference, a `NoInfer<X>` wrapping a still-free
    /// type parameter (the deferred-inference phase). tsc computes such a
    /// callback's return type once, while the parameter is unfixed, and widens
    /// its fresh return literal (`isLiteralOfContextualType` is false for a bare
    /// type variable), then caches and reuses the widened type. tsz re-derives
    /// the return type after the parameter is fixed — when the contextual return
    /// has collapsed to `NoInfer<concrete>` and would spuriously preserve the
    /// literal. Recording the body here lets the post-fix widening decision keep
    /// the widen, matching tsc. An *explicit* type argument never produces the
    /// unfixed phase, so an explicit `NoInfer<"foo">` callback is never recorded
    /// and keeps its literal (#17501).
    pub noinfer_generic_return_bodies: FxHashSet<NodeIndex>,

    /// Deferred TS2454 diagnostics that survive speculative rollback.
    /// `check_flow_usage` can run inside speculative call-checker contexts
    /// (generic inference, overload probing) that truncate diagnostics on
    /// rollback. To prevent TS2454 from being silently discarded, the error
    /// is buffered here and emitted at the end of `check_source_file`.
    pub deferred_ts2454_errors: Vec<(NodeIndex, SymbolId)>,

    /// Nodes where `check_flow_usage` already applied flow narrowing.
    /// The second narrowing pass in `get_type_of_node` must skip these to avoid
    /// double-narrowing (e.g., `any` → `string` → `string & Object`).
    pub flow_narrowed_nodes: CowCache<FxHashSet<u32>>,

    /// `TypeIds` whose lazy/type-query refs have been walked and resolved.
    /// This avoids repeated deep traversals in `ensure_refs_resolved`.
    pub refs_resolved: FxHashSet<TypeId>,

    /// `TypeIds` whose application/lazy symbol references are fully resolved in `type_env`.
    /// This avoids repeated deep traversals in assignability hot paths.
    pub application_symbols_resolved: FxHashSet<TypeId>,

    /// Recursion guard for application symbol resolution traversal.
    pub application_symbols_resolution_set: FxHashSet<TypeId>,

    /// Maps namespace `TypeIds` to their module display name.
    /// Used to display namespace types as `typeof import("module")` instead of
    /// the literal object type shape (e.g., `'{}'`), matching TSC's behavior.
    /// Populated by `NS_CONSTRUCT` in `compute_type_of_symbol`.
    pub namespace_module_names: FxHashMap<TypeId, String>,

    /// Cache for synthesized JS/CommonJS export surfaces per file index.
    /// Avoids redundant re-derivation of export shapes across multiple consumers.
    pub js_export_surface_cache:
        FxHashMap<usize, crate::query_boundaries::js_exports::JsExportSurface>,

    /// Recursion guard for synthesized JS/CommonJS export surfaces.
    /// A present key marks a file whose surface construction is still pending,
    /// preventing self-recursive re-derivation for files that inspect their
    /// own `module.exports` shape. The value carries the file's direct
    /// `module.exports = X` type once step 1 of the computation derives it:
    /// a `module.exports` read typed *during* the rest of the computation
    /// (e.g. inside a sibling `module.exports.p = function () { ... }` RHS
    /// whose body reads `module.exports`) resolves to that instead of the
    /// empty re-entrancy placeholder, matching tsc, which types such reads as
    /// the export= target.
    pub js_export_surface_resolution_set: FxHashMap<usize, Option<TypeId>>,

    /// Recursion guard for JS expando property reads.
    /// Prevents `NS.K = class { return new NS.K() }`-style self-reference loops
    /// from recursively re-evaluating the same expando property via the RHS.
    pub expando_property_resolution_set: CowCache<FxHashSet<String>>,

    /// Maps `file_id` -> module specifier for import-qualified type display.
    /// When a type is defined in a module file, the formatter qualifies its name
    /// as `import("specifier").TypeName` to match tsc's behavior.
    /// Built from the arena's `source_files` during checker construction.
    pub module_specifiers: Arc<FxHashMap<u32, String>>,

    /// Maps `file_id` -> module specifier preserving any directory prefix,
    /// used by diagnostic cross-module disambiguation. tsc's diagnostic output
    /// uses the project-relative path (e.g. `src/library-a/index`) rather
    /// than the basename so that two files sharing the same basename can be
    /// told apart in `import("<path>").X` messages.
    pub module_path_specifiers: Arc<FxHashMap<u32, String>>,

    /// Set when `ProgramContext::apply_to` pre-populated `module_specifiers`
    /// and `module_path_specifiers` from the shared program-level maps. When
    /// true, `set_all_arenas` skips the per-file O(files) rebuild of those two
    /// maps (they are file-independent and identical for every checker), which
    /// removes an O(files^2) term from large-program checking.
    pub module_specifiers_prebuilt: bool,

    /// Maps class instance `TypeIds` to their class declaration `NodeIndex`.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members (and thus no brand).
    /// Populated by `get_class_instance_type_inner` when creating class instance types.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration `NodeIndex` -> computed instance `TypeId`.
    /// Avoids recomputing the full class instance type on every member check.
    ///
    /// Wrapped in `RefCell` for `&self` interior mutability (#12101); see the
    /// matching field on `TypeCache`.
    pub class_instance_type_cache: RefCell<FxHashMap<NodeIndex, TypeId>>,

    /// Forward cache: class declaration `NodeIndex` -> computed constructor `TypeId`.
    /// Avoids recomputing constructor inheritance checks in class-heavy programs.
    ///
    /// Wrapped in `RefCell` for `&self` interior mutability (#12101); see the
    /// matching field on `TypeCache`.
    pub class_constructor_type_cache: RefCell<FxHashMap<NodeIndex, TypeId>>,

    /// Cache for class chain summaries (class declaration `NodeIndex` -> summary).
    /// Avoids recomputing the full inheritance chain member walk on every property
    /// access and override check in class-heavy programs.
    pub(crate) class_chain_summary_cache: RefCell<
        FxHashMap<NodeIndex, std::rc::Rc<crate::classes_domain::class_summary::ClassChainSummary>>,
    >,

    /// Shared evaluation cache for `evaluate_type_with_env` results.
    /// Avoids re-evaluating the same `TypeId` through recursive mapped/conditional
    /// types on every call (e.g., `DeepPartial<Normalize<T>>` accessed 11k+ times
    /// in optional-chain-heavy benchmarks). Analogous to `node_types` for nodes.
    ///
    /// The cache also preserves whether evaluation exceeded the solver recursion
    /// limit so follow-up validation passes can still surface TS2589 from a cache hit.
    pub(crate) env_eval_cache: RefCell<env_eval_cache::EnvEvalCache>,

    /// Memoizes the set of lazy `DefId`s reachable from a type (the structural
    /// `collect_lazy_def_ids` walk). The walk is pure over the immutable interned
    /// type structure, but it is re-run for the same (large, lib-heavy) types
    /// from many hot callers (alias checking, assignability, type analysis,
    /// `lib_augmentations`, the env-eval seed/persist scan), where it dominates
    /// `walk_referenced_types` on real lib-heavy projects. Caching the result is
    /// a pure speed memo: identical to recomputing on demand.
    pub(crate) lazy_def_ids_cache: RefCell<FxHashMap<TypeId, std::rc::Rc<[DefId]>>>,

    /// Memoizes the set of `TypeQuery` (`typeof X`) symbol references reachable
    /// from a type (the structural `collect_type_queries` walk). Like
    /// `lazy_def_ids_cache`, the walk is pure over the immutable interned type
    /// structure, so the result is stable for a `TypeId`. It is re-run for the
    /// same (large, lib-heavy) types from the relation-readiness worklist
    /// (`ensure_refs_resolved`) once per distinct DOM/lib method signature, where
    /// it re-walks the whole signature tree — usually finding zero `typeof`
    /// references — and shows up in `walk_referenced_types` on DOM-heavy files.
    /// Caching (including the common empty result) makes the Nth distinct
    /// method's readiness walk O(1) instead of O(signature size). Pure speed
    /// memo: identical to recomputing on demand.
    pub(crate) type_queries_cache: RefCell<FxHashMap<TypeId, std::rc::Rc<[tsz_solver::SymbolRef]>>>,

    /// Memoizes type-position identifier resolution
    /// (`resolve_identifier_symbol_in_type_position`) keyed by
    /// `(arena pointer, node index)`.
    ///
    /// Recursive type evaluation (e.g. `DeepReadonly<T>` over a deeply-nested
    /// object) re-lowers the same alias-body identifier nodes at every recursion
    /// level, re-resolving each by name through scope walks and the binder's
    /// by-name `HashMap` — the dominant non-lib recompute cliff (issue #13987,
    /// where `resolve_identifier_symbol_in_type_position_inner` was 75.6%
    /// inclusive). The alias/global/namespace/module-augmentation resolution is a
    /// pure function of `(arena, node)` for a fixed binder + lib context — the
    /// same key the binder's own `resolve_identifier` cache uses — so caching it
    /// is a pure speed memo, identical to recomputing on demand.
    ///
    /// Only context-free resolutions are stored: the enclosing-type-parameter
    /// fast path (`resolve_enclosing_type_parameter_symbol`) is *never* cached
    /// here because the same lexical node can bind to different type-parameter
    /// symbols across instantiation/return contexts (the
    /// `return_context_type_param_shadowing_tests` shape). The arena pointer is
    /// per-file, so the cache shares the per-file lifecycle and is cleared on
    /// `reset_for_next_source_file`.
    pub(crate) type_position_resolution_cache:
        RefCell<FxHashMap<(usize, u32), crate::symbol_resolver::TypeSymbolResolution>>,

    /// Memoizes parsed `package.json` payloads (keyed by package-root path) for
    /// the `typesVersions` import-redirect fallback. Without it,
    /// `types_versions_redirected_target_index` re-reads and re-parses the same
    /// `package.json` from disk on every import resolution — a dominant cost on
    /// package-heavy real apps (e.g. nextjs re-resolving `react`/`next`). The
    /// `None` slot caches "no readable/parseable package.json" so misses are not
    /// retried. `package.json` is stable within a compile; reset at the
    /// file-session boundary keeps the LSP picking up edits between compiles.
    pub(crate) package_json_cache:
        RefCell<FxHashMap<std::path::PathBuf, Option<std::rc::Rc<serde_json::Value>>>>,

    /// Cache class symbol -> class declaration node lookups used in inheritance queries.
    /// Stores misses as `None` to avoid repeated declaration scans on hot paths.
    pub class_symbol_to_decl_cache: RefCell<FxHashMap<SymbolId, Option<NodeIndex>>>,

    /// Cache heritage expression node -> resolved symbol lookups.
    /// Stores misses as `None` to avoid repeating namespace/alias walks across
    /// class and interface inheritance passes.
    pub heritage_symbol_cache: RefCell<FxHashMap<NodeIndex, Option<SymbolId>>>,

    /// Cache constructor type fallback for heritage expressions with no explicit type args.
    /// Avoids repeatedly re-evaluating anonymous/complex `extends` expressions.
    pub base_constructor_expr_cache: RefCell<FxHashMap<NodeIndex, Option<TypeId>>>,

    /// Cache instance type fallback for heritage expressions with no explicit type args.
    /// Reuses constructor->instance fallback work across class instance checks.
    pub base_instance_expr_cache: RefCell<FxHashMap<NodeIndex, Option<TypeId>>>,

    /// Cache of non-class `TypeId`s for `get_class_decl_from_type`.
    /// Avoids repeating private-brand scans on hot miss paths.
    pub class_decl_miss_cache: RefCell<FxHashSet<TypeId>>,

    /// Cache for JSX intrinsic element evaluated props types.
    /// Maps (`intrinsic_elements_type`, `tag_atom`) -> `evaluated_props_type`.
    /// Avoids re-evaluating `JSX.IntrinsicElements['div']` for every `<div>` element.
    pub jsx_intrinsic_props_cache: FxHashMap<(TypeId, tsz_common::interner::Atom), TypeId>,

    /// Cached `JSX` namespace symbol for the current checker/file.
    /// Stores both hits and misses to avoid repeated global/lib namespace scans.
    pub jsx_namespace_symbol_cache: Option<Option<SymbolId>>,

    /// Cached `JSX.IntrinsicElements` symbol for the current checker/file.
    /// Stores both hits and misses to avoid repeated namespace export walks.
    pub jsx_intrinsic_elements_symbol_cache: Option<Option<SymbolId>>,

    /// Cached `JSX.IntrinsicElements` type for the current checker/file.
    /// Stores both hits and misses to avoid repeated type-position resolution.
    pub jsx_intrinsic_elements_type_cache: Option<Option<TypeId>>,

    /// Whether TS2875 (JSX import source not found) has been checked for this file.
    /// Set to true after the first JSX element is checked, to emit at most once per file.
    pub jsx_import_source_checked: bool,

    /// Deferred TS2875 diagnostic. Stored here because the check runs inside JSX
    /// element type resolution, which may be inside a speculative call-checker
    /// context that truncates diagnostics. Emitted at end of `check_source_file`.
    pub deferred_jsx_import_source_error: Option<(NodeIndex, String)>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Stack of symbols currently being evaluated for dependency tracking.
    pub symbol_dependency_stack: Vec<SymbolId>,

    /// Set of symbols that have been referenced (used for TS6133 unused checking).
    /// Uses `RefCell` to allow tracking from &self methods (e.g., `resolve_identifier_symbol`).
    pub referenced_symbols: std::cell::RefCell<FxHashSet<SymbolId>>,

    /// Set of symbols written to (assignment targets).
    /// Tracked separately from references for flow/usage checks.
    pub written_symbols: std::cell::RefCell<FxHashSet<SymbolId>>,

    /// Set of class member symbols that have been accessed via property access
    /// (e.g., `this.x`, destructuring of `this`). Populated by
    /// `check_property_accessibility`. Used to determine whether a parameter
    /// property's value was actually read (for TS6138), since the general
    /// `referenced_symbols` set conflates parameter variable references with
    /// property references due to deduplication of symbols sharing the same
    /// declaration node.
    pub referenced_as_property: std::cell::RefCell<FxHashSet<SymbolId>>,

    // --- Destructured Binding Tracking ---
    /// Maps destructured const binding symbols to their source union type info.
    /// Used for correlated discriminant narrowing (TS 4.6+ feature).
    pub(crate) destructured_bindings: FxHashMap<SymbolId, DestructuredBindingInfo>,
    /// Counter for generating unique binding group IDs.
    pub(crate) next_binding_group_id: u32,
    /// Maps destructured binding element symbols to (`source_expression`, `property_name`).
    /// Used for flow narrowing: when `const { bar } = aFoo` and `aFoo.bar` has been
    /// narrowed by a condition, `bar`'s type should use the narrowed property type.
    /// Recorded for ALL destructured bindings, not just union sources.
    pub destructured_binding_sources: FxHashMap<SymbolId, (NodeIndex, String)>,

    // --- Diagnostics ---
    /// Whether the source file has parse errors.
    /// Set by the driver before type checking to suppress noise-sensitive diagnostics
    /// (e.g., TS2695 for comma operators in malformed JSON files).
    pub has_parse_errors: bool,
    /// Whether the source file has real syntax errors (not just conflict markers TS1185).
    /// Used to suppress TS2304 only when there are genuine parse errors.
    pub has_syntax_parse_errors: bool,
    /// Positions (start) of syntax parse errors (excluding conflict markers TS1185).
    /// Used for targeted TS2304 suppression near parse error sites.
    pub syntax_parse_error_positions: Vec<u32>,
    /// Whether the file has "real" syntax errors (TS1005, TS1109, TS1127, TS1128,
    /// TS1135, etc.) that indicate actual parse failure, as opposed to grammar
    /// checks (TS1100, TS1173, TS1212, etc.) which are semantic errors emitted
    /// during parsing. Used for broader TS2304 suppression matching tsc behavior.
    pub has_real_syntax_errors: bool,
    /// Whether the file has structural parse errors -- errors that cause AST
    /// malformation and set `containsParseError` in tsc's parser (TS1005,
    /// TS1068, TS1109, etc.). Unlike `has_real_syntax_errors`, this excludes
    /// grammar-only violations like TS1101 ("with" in strict mode) that don't
    /// affect AST structure. Used for file-wide TS2564 suppression: tsc
    /// suppresses definite-assignment analysis for the entire source file when
    /// any structural parse error exists anywhere in the file.
    pub has_structural_parse_errors: bool,
    /// Positions of "real" syntax errors only (matching `is_real_syntax_error()`).
    /// Used for per-node TS2564 suppression -- only real parse failures (not grammar
    /// checks like TS1030 "modifier already seen") suppress property initialization.
    pub real_syntax_error_positions: Vec<u32>,
    /// Positions of ALL parse errors (including non-suppressing ones like TS1359).
    /// Used for TS2456 suppression when a parse error falls within a type alias.
    pub all_parse_error_positions: Vec<u32>,
    /// Positions of nullable-type parse errors (`?T` / `T?` syntax, TS17019/TS17020).
    /// Used by TS2677 to widen predicate types to `T | null | undefined`.
    /// Excludes `!T` / `T!` errors which should not trigger widening.
    pub nullable_type_parse_error_positions: Vec<u32>,
    /// Half-open `[start, end)` spans of rest parameters whose parser-emitted
    /// grammar diagnostics (TS1014/TS1047/TS1048) must be suppressed because tsc's
    /// single-early-return `checkGrammarParameterList` already reported an
    /// earlier-positioned grammar error on the same parameter list
    /// (TS1015/TS1016) and never reached the rest parameter. Populated by
    /// `check_parameter_ordering`; consumed by the driver's per-file merge
    /// (`run_check_on_existing_checker`) which drops the losing parse
    /// diagnostics before combining them with checker diagnostics.
    pub parameter_grammar_suppress_spans: Vec<(u32, u32)>,
    pub diagnostics: Vec<Diagnostic>,
    /// Whether this context's `diagnostics` are never surfaced to the user.
    ///
    /// Set by [`Self::with_parent_cache`] for every transient cross-arena
    /// delegation child checker (the parent consumes only the delegated
    /// symbol's type and drops the child's diagnostics at teardown — no
    /// construction site merges them back), and by the driver's skipLibCheck
    /// declaration-preparation pass, which likewise drops its checker
    /// diagnostics. When set, the expensive diagnostic *presentation* work —
    /// solver failure-reason elaboration (`explain_failure`), diagnostic type
    /// formatting, related-info chains, and spelling-suggestion candidate
    /// scans — is skipped. Diagnostics routed through
    /// `CheckerContext::error` are still recorded with the correct code and
    /// span, so counting/dedup predicates over that family keep working;
    /// diagnostics routed through `CheckerContext::push_diagnostic` are
    /// dropped wholesale (before the dedup-key insert). This flag must never
    /// change which checks run or what types are computed.
    pub diagnostics_discarded: bool,
    pub(crate) diagnostic_indices: DiagnosticIndices,
    /// Call-expression nodes that resolved to TS2769 during the current
    /// speculative context. Used so overload resolution can reject outer
    /// candidates whose callback bodies contain a failed nested overload even
    /// when the nested diagnostic is rolled back or suppressed for recovery.
    pub no_overload_call_nodes: CowCache<FxHashSet<u32>>,
    /// Callback return-type TS2322 diagnostics that were emitted during
    /// function body checking but may be pruned by arg collection filters.
    /// Stored separately so they can be restored after pruning and used to
    /// suppress the outer TS2345 argument mismatch.
    pub callback_return_type_errors: Vec<Diagnostic>,
    /// Set of modules that have already had TS2307 emitted (prevents duplicate emissions).
    pub modules_with_ts2307_emitted: CowCache<FxHashSet<String>>,
    /// Deferred truthiness diagnostics (TS2872/TS2873) that survive speculative
    /// rollbacks. These are purely syntactic facts emitted during binary
    /// expression evaluation but lost when call-resolution speculation rolls
    /// back the main diagnostics vector. Flushed once per top-level statement.
    pub deferred_truthiness_diagnostics: Vec<Diagnostic>,
    /// Deferred TS7006 diagnostics for callback parameters on excess object
    /// literal properties. These are produced only after EPC proves the
    /// contextual property invalid, and must survive speculative rollback.
    pub deferred_excess_property_implicit_any_diagnostics: Vec<Diagnostic>,

    // --- Recursion Guards ---
    /// Stack of symbols being resolved.
    pub symbol_resolution_stack: Vec<SymbolId>,
    /// O(1) lookup set for symbol resolution stack.
    pub symbol_resolution_set: FxHashSet<SymbolId>,
    /// Type aliases that are part of a circular dependency chain (TS2456).
    /// Populated when `is_direct_circular_reference` detects a cycle — all
    /// members on the resolution stack between the target and the current
    /// alias are marked circular.
    pub circular_type_aliases: FxHashSet<SymbolId>,
    /// Names for which TS2440 (import conflicts with local declaration) was
    /// emitted.  Used to suppress TS2456 for type aliases whose apparent
    /// circularity is caused by the name conflict rather than a real cycle.
    pub import_conflict_names: FxHashSet<String>,
    /// Tracks module specifiers whose namespace types are currently being computed.
    /// Prevents infinite recursion when circular module imports eagerly resolve all exports
    /// (e.g. react's `prop-types` ↔ `react` cycle in react16.d.ts).
    pub module_namespace_resolution_set: FxHashSet<String>,
    /// Maps import `SymbolIds` to their `TYPE_ALIAS` body type, for imports that merge
    /// a type alias with a namespace re-export (e.g., `export type X = ...` + `export * as X from ...`).
    /// Populated during named import resolution in `compute_type_of_symbol`.
    /// Consumed by `type_reference_symbol_type` to return the type alias body in type contexts.
    pub import_type_alias_types: FxHashMap<SymbolId, TypeId>,
    /// VALUE-side type for merged const/type-alias symbols while their alias body lowers.
    pub merged_value_types: FxHashMap<SymbolId, TypeId>,
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: FxHashSet<SymbolId>,
    /// O(1) lookup set for class constructor type resolution to avoid recursion.
    pub class_constructor_resolution_set: FxHashSet<SymbolId>,
    /// Window-scoped partial constructor types published while a class's
    /// constructor type is being computed. Consulted ONLY by value-position
    /// fallbacks (the identifier resolver's `ctor_already_resolving` path,
    /// the mid-resolution base-heritage lookup, and — only when nested
    /// inside a foreign class's window — the constructor-resolution
    /// re-entrancy guard) so nested computations observe the class's
    /// construct-signature arity. Never read in type positions; entries are
    /// removed when the window closes.
    pub window_partial_ctor_types: FxHashMap<SymbolId, TypeId>,
    /// O(1) lookup set for JSDoc `@enum` annotation resolution. Without this
    /// guard, `/** @enum {E} */ const E = { ... }` recurses through name
    /// resolution → `resolve_jsdoc_symbol_type(E)` → `@enum` annotation lookup
    /// → name resolution again, overflowing the stack (#3767).
    pub jsdoc_enum_resolution_set: FxHashSet<SymbolId>,
    /// Classes/interfaces with circular inheritance (TS2506/TS2310). Used to
    /// fix the return type of `new` on circular generic classes: tsc returns
    /// `C<unknown>` instead of the raw `C<T>`.
    pub circular_class_symbols: FxHashSet<SymbolId>,
    /// Deferred implicit-any candidates keyed by variable symbol.
    /// Bare implicit-any declarations stay capture-only, while evolving empty
    /// arrays also report on unsafe same-scope reads.
    pub pending_implicit_any_vars: CowCache<FxHashMap<SymbolId, PendingImplicitAnyVar>>,
    /// Closure/function-expression sites whose return expressions read a variable
    /// symbol currently being resolved. Used to centralize TS7022/TS7023/TS7024
    /// emission and suppress downstream relation noise from the circularity.
    pub pending_circular_return_sites: PendingCircularReturnSites,
    /// Extra tracking depth for method/accessor return-site circularity when a
    /// construct consults those bodies immediately during type computation
    /// (currently the `for...of` iterator protocol path).
    pub non_closure_circular_return_tracking_depth: usize,
    /// Per-function memo of inferred body return types (tsc's
    /// `resolvedReturnType`). Keyed by the function/arrow/method node plus every
    /// ambient input that determines pure return-type inference: the contextual
    /// return type, the literal-preservation flags, and a fingerprint of the
    /// active type-parameter scope. It records the inferred return `TypeId` only,
    /// never any diagnostics — body checking with diagnostics runs separately and
    /// is never short-circuited by this cache. The memo is consulted only when
    /// the function is NOT participating in circular return resolution, so a
    /// provisional (in-progress) inference is never published. Cleared per file
    /// session like the other return-circularity caches.
    pub inferred_return_type_memo: FxHashMap<InferredReturnTypeKey, TypeId>,
    /// Memo for the contextual-callback return-type mismatch derivation
    /// (`raw_block_body_callback_mismatch`). Keyed by the inline callback
    /// argument node and the expected contextual type for that argument. The
    /// value is the stable mismatch outcome `(arg index, recovery actual,
    /// expected)`, or `None` when no mismatch is forced. Overload resolution and
    /// the union/signature-specific passes re-enter the derivation for the same
    /// pair many times; the derivation snapshots and rolls back all speculative
    /// state it touches, so its result is a pure function of the key. Only
    /// non-generator callbacks are recorded (generator callbacks additionally
    /// depend on the durable diagnostic vector). Cleared per file session like
    /// `inferred_return_type_memo`.
    pub callback_mismatch_memo: CallbackMismatchMemo,
    /// Variables that have already had TS7034 emitted, keyed by the deferred
    /// implicit-any classification that triggered it.
    pub reported_implicit_any_vars: CowCache<FxHashMap<SymbolId, PendingImplicitAnyKind>>,

    /// Inheritance graph tracking class/interface relationships
    pub inheritance_graph: tsz_solver::classes::inheritance::InheritanceGraph,

    /// Stack of nodes being resolved.
    /// Also used for circular reference detection via linear scan (the stack is
    /// typically 0-5 elements deep, where linear scan beats `FxHashSet` lookup).
    pub node_resolution_stack: Vec<NodeIndex>,

    /// Closures where implicit any (TS7006/TS7031) checks have already been performed.
    /// Prevents duplicate diagnostics when `get_type_of_function` is called multiple
    /// times for the same closure (e.g., once with contextual type during call
    /// resolution, then again without context during body checking).
    pub implicit_any_checked_closures: CowCache<FxHashSet<NodeIndex>>,
    /// Closures that have already been checked with a real contextual parameter type.
    /// Preserve this across cache clears so later context-free rechecks do not
    /// emit false TS7006/TS7031 diagnostics.
    pub implicit_any_contextual_closures: CowCache<FxHashSet<NodeIndex>>,
    /// Closures that were processed during type env building without contextual types.
    /// These closures deferred TS7006 checking (because `is_checking_statements` was false).
    /// After `is_checking_statements` is set to true, these closures need a re-check
    /// because their cached types prevent `get_type_of_function` from re-running.
    pub deferred_implicit_any_closures: Vec<NodeIndex>,

    /// Closures whose TS7006 was emitted during return-type inference speculation
    /// and then rolled back. These need re-checking after all call inference is
    /// complete to determine if TS7006 should be in the final output.
    /// Only closures NOT in `implicit_any_contextual_closures` (i.e., those that
    /// never received contextual parameter types) will have TS7006 re-emitted.
    pub speculative_implicit_any_closures: Vec<NodeIndex>,

    /// Closures (function expressions and arrow functions) that have a contextual
    /// `this` type from their parent call expression's parameter type.
    /// Used to suppress TS2683 errors for callback functions with contextual this types.
    pub closures_with_contextual_this_type: FxHashSet<NodeIndex>,

    /// Set of class declaration nodes currently being checked.
    /// Used to prevent infinite recursion in `check_class_declaration` when
    /// class checking triggers type resolution that circles back to the same class.
    pub checking_classes: FxHashSet<NodeIndex>,

    /// Set of class declaration nodes that have been fully checked.
    /// Used to avoid re-checking the same class multiple times (e.g. once via
    /// dependency resolution and once via the main source file traversal).
    pub checked_classes: FxHashSet<NodeIndex>,

    // --- Scopes & Context ---
    /// Current type parameter scope.
    pub type_parameter_scope: FxHashMap<String, TypeId>,

    /// Checker-local memos for type-reference argument validation.
    pub type_reference_validation_caches: TypeReferenceValidationCaches,

    /// Depth counter for conditional type `extends` clauses.
    /// Incremented when recursing into the `extends_type` of a conditional type,
    /// used to validate TS1338: `infer` only allowed in conditional extends.
    pub in_conditional_extends_depth: u32,

    /// Temporary scope for value parameters visible to `typeof` in return type annotations.
    /// Populated during signature processing so `typeof paramName` in return types
    /// can resolve to the parameter's type.
    pub typeof_param_scope: FxHashMap<String, TypeId>,

    /// Parameter names excluded from `typeof` resolution in type parameter constraints.
    /// When processing type parameter constraints for a function/method/constructor,
    /// the function's own value parameters are NOT in scope for `typeof paramName`.
    pub type_param_constraint_excluded_params: FxHashSet<String>,

    /// Contextual type for expression being checked.
    pub contextual_type: Option<TypeId>,

    /// When true, the contextual type originates from a type assertion (`as` or
    /// `<T>` cast).  In that case parameter types are contextually typed but the
    /// function body's return type should NOT be checked against the contextual
    /// return type — only TS2352 is emitted at the assertion site.
    pub contextual_type_is_assertion: bool,

    /// Whether we're in the statement checking phase (vs type environment building).
    /// During `build_type_environment`, closure parameter types may not have contextual types
    /// yet, so TS7006 should be deferred until the checking phase.
    pub is_checking_statements: bool,

    /// Whether the current file is a declaration file (.d.ts/.d.tsx/.d.mts/.d.cts).
    /// Used to suppress statement-specific errors (TS1105, TS1108, TS1104) in favor of TS1036.
    pub is_in_ambient_declaration_file: bool,

    /// Whether we are currently evaluating the LHS of a destructuring assignment.
    /// Used to suppress TS1117 (duplicate property) checks in object patterns.
    pub in_destructuring_target: bool,

    /// Whether a non-literal array destructuring initializer is being recomputed
    /// for iterability diagnostics. In that path, nested overload failures affect
    /// the initializer type and should survive contextual argument rollback.
    pub preserve_destructuring_initializer_overload_diagnostics: bool,

    /// Whether to skip flow narrowing when computing types.
    /// Used in assignment target type resolution to get declared types instead of narrowed types.
    /// When checking `foo[x] = 1` after `if (foo[x] === undefined)`, we need the declared type
    /// (e.g., `number | undefined`) not the narrowed type (e.g., `undefined`).
    pub skip_flow_narrowing: bool,

    /// Current depth of recursive type instantiation.
    pub instantiation_depth: Cell<u32>,

    /// Whether type instantiation depth was exceeded (for TS2589 emission).
    pub depth_exceeded: Cell<bool>,

    /// Relation-check overflow state observed during assignability/subtype
    /// checks that feed diagnostics.
    pub relation_overflow: Cell<RelationOverflowFlags>,

    /// When true, `should_suppress_assignability_diagnostic` skips the callable-
    /// with-type-params suppression. Set by variable declaration checking to
    /// avoid hiding genuine TS2322 errors for callable types with outer-scope
    /// type parameters assigned to concrete callable targets.
    pub skip_callable_type_param_suppression: Cell<bool>,

    /// Explicit evaluation session state (replaces thread-local depth/fuel guards).
    /// Shared via `Rc` across parent/child contexts so counters survive cross-arena
    /// delegation without implicit global state.
    pub eval_session: Rc<tsz_solver::evaluation::session::EvaluationSession>,

    /// General recursion depth counter for type checking.
    /// Prevents stack overflow by bailing out when depth exceeds the limit.
    pub recursion_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Dedicated depth counter for interface heritage merge recursion.
    /// Heritage merging is expensive per level (resolves full interface types),
    /// so it needs a tighter limit than the general recursion counter.
    pub heritage_merge_depth: Cell<u32>,

    /// Current depth of call expression resolution.
    pub call_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Depth counter for `is_direct_circular_reference` recursion.
    /// Prevents stack overflow when evaluating recursive type aliases
    /// (e.g., `type N<T, K> = T | { [P in K]: N<T, K> }[K]`).
    pub circ_ref_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Depth counter for `types_have_no_overlap` / `objects_with_independently_overlapping_props`
    /// mutual recursion. Prevents stack overflow on infinitely-expanding recursive types
    /// (e.g., `interface List<T> { owner: List<List<T>> }`).
    pub overlap_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Names of `@typedef` types currently being resolved.
    /// Prevents infinite recursion for circular JSDoc typedefs
    /// (e.g., `@typedef {string | JsonArray} Json` where `JsonArray = ReadonlyArray<Json>`).
    pub resolving_jsdoc_typedefs: RefCell<Vec<String>>,

    /// Current anchor position for typedef scoping during JSDoc type resolution.
    /// When set, `resolve_jsdoc_type_name` will skip typedefs that are inside a
    /// function body that doesn't contain this position.
    /// `u32::MAX` means no scoping (global search).
    pub jsdoc_typedef_anchor_pos: std::cell::Cell<u32>,

    /// Stack of expected return types for functions.
    pub return_type_stack: Vec<TypeId>,
    /// Stack of contextual yield types for generator functions.
    /// Used to contextually type yield expressions (prevents false TS7006).
    pub yield_type_stack: Vec<Option<TypeId>>,
    pub generator_next_type_stack: Vec<Option<TypeId>>,
    /// Collected yield operand types during body check for unannotated generators.
    /// After body check, the union determines the inferred yield type for TS7055/TS7025 vs TS7057.
    pub generator_yield_operand_types: Vec<GeneratorYieldContribution>,
    /// Whether TS7057 was emitted for any yield in the current generator.
    /// When true, TS7055 is suppressed (tsc emits one or the other, not both).
    pub generator_had_ts7057: bool,
    /// Stack of current `this` types for the active traversal path.
    pub this_type_stack: Vec<TypeId>,
    /// Functions whose own explicit/contextual `this` pushed the active `this` type.
    ///
    /// This lets `this` resolution distinguish a function-owned binding from an
    /// ambient outer `this` such as a containing class method.
    pub function_owned_this_stack: Vec<NodeIndex>,

    /// Current enclosing class info.
    pub enclosing_class: Option<EnclosingClassInfo>,

    /// Stack of outer enclosing class `NodeIndex`es, from outermost to innermost.
    /// When entering a nested class, the outer class is pushed here.
    /// Used by protected member access checks (TS2446) to find the correct
    /// enclosing class in the inheritance hierarchy when code is inside nested classes.
    pub enclosing_class_chain: Vec<NodeIndex>,

    /// Evaluator `TypeEnvironment` (#13086) — the authoritative env.
    ///
    /// The second of the context's two `TypeEnvironment` instances. The
    /// evaluator/`state`/`types`/`assignability` paths use this one to resolve
    /// symbols and expand `Application` types; it is the source of truth for
    /// `DefId -> TypeId` resolution. `register_*_in_envs` write it directly and
    /// mirror into [`Self::type_environment`]. See that field's doc for why the
    /// two are kept as separate `RefCell`s rather than unified into one.
    pub type_env: RefCell<TypeEnvironment>,

    // --- DefId Migration Infrastructure ---
    /// Storage for type definitions (interfaces, classes, type aliases).
    /// Part of the `DefId` migration to decouple Solver from Binder.
    pub definition_store: Arc<DefinitionStore>,

    /// Whether per-run type results should be mirrored into the shared
    /// `DefinitionStore` result caches.
    ///
    /// Batch project checking enables this through `ProgramContext`; interactive/LSP
    /// callers keep it disabled so persistent shared stores do not accumulate
    /// speculative request-local results across editor operations.
    pub share_owner_symbol_type_results: bool,

    /// Mapping from Binder `SymbolId` to Solver `DefId`.
    /// Used during migration to avoid creating duplicate `DefIds` for the same symbol.
    /// Wrapped in `RefCell` to allow mutation through shared references (for use in Fn closures).
    pub symbol_to_def: RefCell<FxHashMap<SymbolId, DefId>>,

    /// Reverse mapping from Solver `DefId` to Binder `SymbolId`.
    /// Used to look up binder symbols from DefId-based types (e.g., namespace exports).
    /// Wrapped in `RefCell` to allow mutation through shared references (for use in Fn closures).
    pub def_to_symbol: RefCell<FxHashMap<DefId, SymbolId>>,

    /// Type parameters for `DefIds` (used for type aliases, classes, interfaces).
    /// Enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    /// Wrapped in `RefCell` to allow mutation through shared references.
    pub def_type_params: RefCell<FxHashMap<DefId, Vec<tsz_solver::TypeParamInfo>>>,

    /// `DefIds` known to have no type parameters.
    /// This avoids repeated cross-arena lookups for non-generic symbols.
    pub def_no_type_params: RefCell<FxHashSet<DefId>>,

    /// Counter for DefId fallback firings (Step 4 of `get_or_create_def_id`).
    ///
    /// Tracks how many times the checker had to create a DefId on demand
    /// because the symbol was not pre-populated from binder `semantic_defs`.
    /// A growing count indicates gaps in binder semantic def coverage.
    /// Use `def_fallback_count()` to read the value.
    pub def_fallback_count: Cell<u32>,

    /// Whether `warm_local_caches_from_shared_store` has already completed.
    /// Avoids redundant iteration over all symbol mappings when the method
    /// is called multiple times (e.g., once in the constructor and again in
    /// `check_source_file`).
    pub local_caches_warmed: Cell<bool>,

    /// Abstract constructor types (`TypeIds`) produced for abstract classes.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Protected constructor types (`TypeIds`) produced for protected constructors.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Private constructor types (`TypeIds`) produced for private constructors.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// Maps dynamically-discovered cross-file `SymbolIds` to their source file index.
    ///
    /// Child checkers inherit this as a parent+delta snapshot rather than cloning
    /// the full overlay map on every cross-arena delegation.
    pub cross_file_symbol_targets: RefCell<SymbolFileTargetsOverlay>,

    /// Shared base map: `SymbolId` → owning file index (pre-built from `ProgramContext`).
    ///
    /// Cloned as `Arc` (O(1)) when creating child checkers, avoiding the O(N) clone
    /// of `cross_file_symbol_targets`. Read sites use `resolve_symbol_file_index()`
    /// which checks the local overlay first, then this shared base.
    pub global_symbol_file_index: Option<Arc<FxHashMap<SymbolId, usize>>>,

    /// All arenas for cross-file resolution (indexed by `file_idx` from `Symbol.decl_file_idx`).
    /// Set during multi-file type checking to allow resolving declarations across files.
    pub all_arenas: Option<Arc<Vec<Arc<NodeArena>>>>,

    /// All binders for cross-file resolution (indexed by `file_idx`).
    /// Enables looking up exported symbols from other files during import resolution.
    pub all_binders: Option<Arc<Vec<Arc<BinderState>>>>,

    /// Pre-built global index: symbol name -> list of (`file_idx`, SymbolId).
    /// Constructed once in `set_all_binders` from all binders' `file_locals`.
    /// Eliminates O(N) scans in `resolve_identifier_symbol_from_all_binders`
    /// and related cross-file symbol lookup hot paths.
    pub global_file_locals_index: Option<GlobalFileLocalsIndex>,

    /// Pre-built global index: (`module_specifier`, `export_name`) -> list of (`file_idx`, SymbolId).
    /// Constructed once in `set_all_binders` from all binders' `module_exports`.
    /// Eliminates O(N) scans in `resolve_import_from_ambient_module`.
    pub global_module_exports_index: Option<GlobalModuleExportsIndex>,

    /// Pre-built global index of all declared/ambient module names across all binders.
    /// Split into exact names (O(1) lookup) and wildcard patterns (small linear scan).
    /// Eliminates O(N*M) scans in `any_ambient_module_declared`.
    pub global_declared_modules: Option<Arc<GlobalDeclaredModules>>,

    /// Pre-built global index: `obj_key` -> {`property_names`} merged from all binders'
    /// `expando_properties`. Eliminates O(N) scans when checking whether an expando
    /// property exists across any file (`property_access_helpers`, access computation).
    pub global_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>>,

    /// Pre-built global index: `module_specifier` -> Vec<(`file_idx`, `ModuleAugmentation`)>.
    /// Merges all binders' `module_augmentations` into a single lookup table.
    /// Eliminates O(N) scans when resolving module augmentations for interface
    /// declaration merging (`interface_type.rs`, computed.rs).
    pub global_module_augmentations_index: Option<GlobalModuleAugmentationsIndex>,

    /// Lazily-built per-checker candidate list for the duplicate-identifier
    /// global-scope conflict scan: `(file_idx, top-level statement)` pairs that
    /// are `export as namespace X` declarations or `declare global { … }`
    /// blocks in *any* file. The per-symbol-name scan previously walked every
    /// arena's top-level statements once per distinct name per file; almost
    /// all files contain neither construct, so this collapses those walks to
    /// one pass per checker. Entries preserve (file, statement) order.
    pub global_scope_conflict_candidates: OnceCell<Vec<(usize, tsz_parser::parser::NodeIndex)>>,

    /// Memo for `effective_jsx_mode`, keyed by the file index it was computed
    /// for (child checkers and session resets re-point `current_file_idx`).
    /// The mode is a pure function of the file's source text and compiler
    /// options; without the memo every consumer re-scans the source for the
    /// `@jsxRuntime` pragma.
    pub effective_jsx_mode_cache: Cell<Option<(usize, tsz_common::checker_options::JsxMode)>>,

    /// Pre-built global index: `module_specifier` -> Vec<(SymbolId, `file_idx`)>.
    /// Merges all binders' `augmentation_target_modules` (reverse map: symbol -> module)
    /// into a forward lookup: module -> symbols. Eliminates O(N) scans when finding
    /// augmentation symbols for a given module specifier (`interface_type.rs`).
    pub global_augmentation_targets_index: Option<GlobalAugmentationTargetsIndex>,

    /// Pre-built global index: module name -> list of binder indices that have that module
    /// in their `module_exports`. Eliminates O(N) binder scans when looking up which
    /// file(s) declared a given ambient module. Both raw and normalized (quote-stripped)
    /// forms of each module name are indexed.
    pub global_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>>,

    /// Pre-built arena-pointer → file-index map. Eliminates O(N) scans in
    /// `get_binder_for_arena` / `get_file_idx_for_arena` (13+ call sites).
    /// Key is `Arc::as_ptr(arena) as usize` for `Send`/`Sync` safety.
    pub global_arena_index: Option<Arc<FxHashMap<usize, usize>>>,

    /// Normalized-file-name → file-index reverse index consumed by
    /// `resolve_import_target_from_file` via `resolve_specifier_via_file_index`.
    pub global_file_name_index: Option<Arc<crate::module_resolution::FileNameIndex>>,

    /// Program-wide `FileReexportsMap` shared across cross-file lookup
    /// binders. The full merged map lives here once; per-file cross-file
    /// lookup binders leave their `reexports` field empty and the
    /// `ctx.reexports_for_file` accessor consults this first. Avoids the
    /// O(N · `map_size`) deep clone that used to materialize a copy of the
    /// program-wide map into every one of N cross-file binders.
    pub program_reexports: Option<Arc<tsz_binder::FileReexportsMap>>,
    /// Program-wide wildcard re-exports map; see `program_reexports`.
    pub program_wildcard_reexports: Option<Arc<tsz_binder::WildcardReexportsMap>>,
    /// Program-wide module-exports index keyed by file name (or ambient
    /// module specifier). Consulted by `ctx.module_exports_for_module`
    /// in preference to per-binder `module_exports`. Driver wraps
    /// `program.module_exports` in a single `Arc` so N cross-file lookup
    /// binders don't each deep-clone the merged map.
    pub program_module_exports: Option<Arc<FxHashMap<String, tsz_binder::SymbolTable>>>,
    /// Program-wide cross-file node-symbol map keyed by arena pointer.
    /// Consulted by `ctx.cross_file_node_symbols_for_arena` in preference
    /// to per-binder `cross_file_node_symbols`. Driver wraps
    /// `program.cross_file_node_symbols` in a single `Arc` so N per-file
    /// binders don't each deep-clone the outer `FxHashMap<usize, Arc<…>>`.
    pub program_cross_file_node_symbols: Option<Arc<tsz_binder::CrossFileNodeSymbols>>,
    /// Program-wide alias-partners map; consulted by
    /// `ctx.alias_partner_for` in preference to per-binder `alias_partners`.
    /// Driver wraps `program.alias_partners` in a single `Arc`.
    pub program_alias_partners: Option<Arc<FxHashMap<SymbolId, SymbolId>>>,

    /// Resolved module paths map: (`source_file_idx`, specifier) -> `target_file_idx`.
    /// Used by `get_type_of_symbol` to resolve imports to their target file and symbol.
    ///
    /// Key invariant: all specifier lookups should use
    /// `module_resolution::module_specifier_candidates` for canonical variants.
    pub resolved_module_paths: Option<Arc<ResolvedModulePathMap>>,
    /// Resolved module paths keyed by the full driver request, including any
    /// explicit `resolution-mode` override from import attributes / import types.
    pub resolved_module_request_paths: Option<Arc<ResolvedModuleRequestPathMap>>,
    /// `resolvedUsingTsExtension` flag per resolved import. See
    /// [`ResolvedModuleTsExtensionMap`] — consulted by the TS2877 emission gate
    /// to suppress the diagnostic when the package author's `exports`/`imports`
    /// entry literally consumes the `.ts` extension.
    pub resolved_module_ts_extension_flags: Option<Arc<ResolvedModuleTsExtensionMap>>,

    /// Current file index in multi-file mode (index into `all_arenas/all_binders`).
    /// Used with `resolved_module_paths` to look up cross-file imports.
    pub current_file_idx: usize,

    /// Per-`source_file_idx` cache: does this file contain a type-position
    /// `import(...)` call whose `assert` option is itself TS2880-eligible?
    /// tsc suppresses TS2880 on every value-position (dynamic import) `assert`
    /// occurrence in a file whenever a type-position sibling occurrence
    /// exists — see the file-wide suppression gate in
    /// `declarations/dynamic_import_checker.rs`. Computed once per file by a
    /// syntactic pre-scan at the start of `check_source_file`.
    pub type_position_deprecated_import_assert_files: FxHashMap<usize, bool>,

    /// Deterministic allocator state for inference-placeholder names, packed as
    /// `(file_namespace << 32) | next_sequence`.
    ///
    /// The high 32 bits are captured from `current_file_idx` at the start of
    /// each file check (see `begin_file_inference_placeholders`) so `__infer_*`
    /// names stay program-unique even under parallel file checking, while
    /// staying independent of the temporary `current_file_idx` switches that
    /// cross-file import resolution performs. The low 32 bits are a per-file
    /// counter, reset each file so the names a given file produces are
    /// identical on every run regardless of thread scheduling or how many
    /// placeholders other files allocated.
    pub inference_placeholder_state: Cell<u64>,

    /// Resolved module specifiers for this file (multi-file CLI mode).
    ///
    /// Wrapped in `Arc` so the CLI per-file driver can share the
    /// pre-bucketed `resolved_modules_per_file[file_idx]` entry without
    /// deep-cloning the `FxHashSet<String>` contents per file. Child
    /// checkers (cross-arena delegation) bump the refcount.
    pub resolved_modules: Option<Arc<FxHashSet<String>>>,

    /// Track value exports declared in module augmentations for duplicate detection.
    /// Keyed by a canonical module key (resolved file index or specifier).
    pub module_augmentation_value_decls: FxHashMap<String, FxHashMap<String, NodeIndex>>,

    /// Recursion guard for module augmentation application.
    /// Prevents infinite re-entry when applying the same augmentation to the
    /// same base type through callable/prototype or lazy-evaluation loops.
    pub module_augmentation_application_set: RefCell<FxHashSet<(String, String, TypeId)>>,

    /// Per-file cache of `is_external_module` values to preserve state across files.
    /// Maps file path -> whether that file is an external module (has imports/exports).
    /// This prevents state corruption when binding multiple files sequentially.
    pub is_external_module_by_file: Option<Arc<FxHashMap<String, bool>>>,

    /// Map of resolution errors: (`source_file_idx`, specifier) -> Error details.
    /// Populated by the driver when `ModuleResolver` returns a specific error.
    /// Contains structured error information (code, message) for TS2834, TS2835, TS2792, etc.
    ///
    /// Diagnostic-source invariant: module-not-found-family code/message selection
    /// should come from resolver outcomes when present.
    pub resolved_module_errors: Option<Arc<ResolvedModuleErrorMap>>,
    /// Resolution errors keyed by the full driver request, including any
    /// explicit `resolution-mode` override from import attributes / import types.
    pub resolved_module_request_errors: Option<Arc<ResolvedModuleRequestErrorMap>>,
    /// Untyped-JavaScript resolution targets: (`source_file_idx`, specifier) ->
    /// resolved `.js` path. Consulted by the module-augmentation TS2665 gate.
    pub untyped_module_paths: Option<Arc<UntypedModulePathMap>>,

    /// Import resolution stack for circular import detection.
    /// Tracks the chain of modules being resolved to detect circular dependencies.
    pub import_resolution_stack: Vec<String>,

    /// Set of import specifier nodes that should be elided from JavaScript output.
    /// These are imports that reference type-only declarations (interfaces, type aliases).
    /// Populated during type checking and consulted by the emitter.
    pub type_only_nodes: FxHashSet<NodeIndex>,

    /// Symbol resolution depth counter for preventing stack overflow.
    /// Tracks how many nested `get_type_of_symbol` calls we've made.
    pub symbol_resolution_depth: Cell<u32>,

    /// Maximum symbol resolution depth before we give up (prevents stack overflow).
    /// Default value: [`MAX_SYMBOL_RESOLUTION_DEPTH`].
    pub max_symbol_resolution_depth: u32,

    /// Lib file contexts for global type resolution (lib.es5.d.ts, lib.dom.d.ts, etc.).
    /// Each entry is a (arena, binder) pair from a pre-parsed lib file.
    /// Used as a fallback when resolving type references not found in the main file.
    ///
    /// Wrapped in `Arc` so that child checkers (cross-file delegation) and parallel
    /// per-file checkers can share the same lib contexts with O(1) clone cost instead
    /// of cloning the entire Vec<LibContext> (which requires N Arc increments per clone).
    pub lib_contexts: Arc<Vec<LibContext>>,

    /// Pre-computed lib binders extracted from `lib_contexts`.
    /// Avoids repeated `Vec<Arc<BinderState>>` allocation + Arc cloning on every
    /// symbol resolution call (called thousands of times per file).
    ///
    /// Wrapped in `Arc` for the same O(1) sharing reasons as `lib_contexts`.
    pub lib_binders_cached: Arc<Vec<Arc<tsz_binder::BinderState>>>,

    /// Union of every name declared in any `lib_contexts[*].binder.file_locals`,
    /// precomputed once and shared (`Arc`) across all per-file checkers.
    ///
    /// Type/value-position identifier resolution probes `lib_contexts` directly
    /// for module-scoped lib symbols the binder merge intentionally excludes
    /// (see `resolve_identifier_symbol*`). That probe is an `O(num_lib_files)`
    /// scan of `file_locals` run per unresolved identifier. The overwhelming
    /// majority of names a project resolves (its own type aliases, generics,
    /// imports) are absent from every lib, so the scan is a guaranteed no-op for
    /// them. Gating the scan on membership in this set turns the common case
    /// into one hash lookup while staying byte-identical: a name absent from the
    /// set cannot match any `file_locals.get(name)`, so skipping the loop is
    /// exactly equivalent to running it and finding nothing.
    ///
    /// `None` means the index was not built for this context (e.g. a child
    /// checker created without it); callers then fall back to the full scan, so
    /// resolution stays correct regardless of whether the index is present.
    pub lib_file_local_names: Option<Arc<rustc_hash::FxHashSet<String>>>,

    /// Number of actual lib files loaded (not including user files).
    /// Used by `has_lib_loaded()` to correctly determine if standard library is available.
    /// This is separate from `lib_contexts.len()` because `lib_contexts` may also include
    /// user file contexts for cross-file type resolution in multi-file tests.
    pub actual_lib_file_count: usize,

    /// Whether the driver loaded a project-local `@typescript/lib-dom` replacement package.
    /// Used to report plain unresolved-name errors for omitted DOM globals like `window`.
    pub typescript_dom_replacement_loaded: bool,
    /// Whether the loaded replacement package explicitly declares a global `window` value.
    pub typescript_dom_replacement_has_window: bool,
    /// Whether the loaded replacement package explicitly declares a global `self` value.
    pub typescript_dom_replacement_has_self: bool,

    /// Control flow graph for definite assignment analysis and type narrowing.
    /// This is built during the binding phase and used by the checker.
    pub flow_graph: Option<FlowGraph<'a>>,

    /// Async context depth - tracks nesting of async functions.
    /// Used to check if await expressions are within async context (TS1359).
    pub async_depth: u32,

    /// Stack of symbols being resolved via typeof to detect cycles.
    /// Prevents infinite loops in typeof X where X's type computation depends on typeof X.
    pub typeof_resolution_stack: RefCell<FxHashSet<u32>>,

    /// Stack of symbols whose cross-file type-parameter omitted-default
    /// constraints are currently being expanded. A type parameter constraint
    /// that is a bare generic reference (`<P extends Ref>`) is re-expanded with
    /// `Ref`'s own defaults, which re-enters `get_reference_type_params_for_symbol`
    /// for `Ref`; when that chain leads back to a symbol already on the stack
    /// (mutually or self-referential generic registries, e.g. the fp-ts HKT
    /// `URItoKind`/`Kind` family), the expansion must stop rather than recurse
    /// without bound. The result cache only breaks the cycle once a frame has
    /// fully returned, so a cold cache (fresh per-file check) needs this guard.
    pub omitted_default_constraint_stack: RefCell<FxHashSet<u32>>,

    /// Closure depth - tracks nesting of function expressions, arrow functions, and method expressions.
    /// Used to apply Rule #42: CFA Invalidation in Closures.
    /// When > 0, mutable variables (let/var) lose narrowing in closures.
    pub inside_closure_depth: u32,

    /// When true, we're inside a const assertion (as const) and should preserve literal types.
    /// This prevents widening of literal types in object/array literals.
    pub in_const_assertion: bool,

    /// True while checking a `satisfies T` operand. Object-literal property
    /// widening then uses tsc's exact `isLiteralOfContextualType` per-property
    /// gate instead of tsz's normal coarser policy.
    pub in_satisfies_operand: bool,

    /// Preserve literal types instead of widening. Set during compound
    /// expression branches (conditional `?:`, logical `||`/`&&`/`??`).
    pub preserve_literal_types: bool,
    /// Preserve primitive literal operands for logical `const` initializers.
    pub preserve_logical_operand_literals: bool,
    /// When true, identifier resolution should return the symbol's declared
    /// type (when one is explicitly annotated) rather than a flow-narrowed
    /// type. Set during class property initializer evaluation so that
    /// `class C { D = DEFAULT; }` (where `const DEFAULT: AB = 'A'`) infers
    /// `D: AB`, not `D: 'A'` (the flow-narrowed value type) — matching tsc.
    pub use_declared_type_for_identifier: bool,

    /// When true, array literals skip the contextual supertype collapse.
    /// Set during `yield*` expression computation so that `[new Bar]` with
    /// contextual type `Foo[]` (where `Bar extends Foo`) produces `Bar[]`
    /// instead of `Foo[]`. This ensures the inferred generator yield type
    /// reflects the actual yielded values, not the contextual supertype.
    pub skip_array_contextual_supertype_collapse: bool,

    /// Per-argument mask indicating which call arguments should skip excess
    /// property checking because the original (pre-instantiation) parameter type
    /// is or contains a type parameter.
    ///
    /// For generic function calls like `parrot<T extends Named>({name, age})`,
    /// the instantiated parameter type is the constraint `Named = {name: string}`,
    /// which would cause a false TS2353 for `age`. But tsc skips excess property
    /// checks when the parameter type is a type parameter because `T` captures
    /// the full object type.
    ///
    /// Set before `collect_call_argument_types_with_context` for generic calls
    /// and cleared afterwards.
    pub generic_excess_skip: Option<Vec<bool>>,

    // --- Control Flow Validation ---
    /// Depth of nested iteration statements (for/while/do-while).
    /// Used to validate break/continue statements.
    pub iteration_depth: u32,

    /// Depth of nested switch statements.
    /// Used to validate break statements (break is valid in switch).
    pub switch_depth: u32,

    /// Depth of nested functions.
    /// Used to detect when labeled jumps cross function boundaries.
    pub function_depth: u32,

    /// The [`Self::function_depth`] at which the innermost enclosing class
    /// member body (method, constructor, accessor, or static block) was
    /// entered; `0` when no class member body is being checked.
    ///
    /// A class member body is a function boundary — it raises
    /// `function_depth` like any other function body — but the
    /// abstract-property-access checks (TS2715) need the narrower question
    /// "is this statement directly in the member body, or in a function
    /// nested inside it?". Comparing against this baseline answers that
    /// without a second notion of "function".
    pub(crate) class_member_body_depth: u32,

    /// Track whether current code path is syntactically unreachable.
    pub is_unreachable: bool,

    /// Track whether we have already reported an unreachable error in this block/scope.
    pub has_reported_unreachable: bool,

    /// TS7027 subtree suppression, mirroring tsc's `withinUnreachableCode`:
    /// once a statement is covered by a reported unreachable range, every
    /// statement beneath it (namespace bodies, class member bodies, nested
    /// blocks) stays TS7027-silent while normal checking continues.
    /// Scoped per statement by `StatementChecker::check_with_request`.
    pub suppress_unreachable_reporting: bool,

    /// Stack of labels in scope.
    /// Each entry contains (`label_name`, `is_iteration`, `function_depth_when_defined`).
    /// Used for labeled break/continue validation.
    pub(crate) label_stack: Vec<LabelInfo>,

    /// Whether there was a loop/switch in an outer function scope.
    /// Used to determine TS1107 vs TS1105 for unlabeled break statements.
    /// When true, an unlabeled break inside a function should emit TS1107,
    /// because the break is "trying" to exit the outer loop but can't cross
    /// the function boundary.
    pub had_outer_loop: bool,

    /// When true, suppress definite assignment errors (TS2454).
    /// This is used during return type inference to avoid duplicate errors.
    /// The function body is checked twice: once for return type inference
    /// and once for actual statement checking. We only want to emit TS2454
    /// errors during the second pass.
    pub suppress_definite_assignment_errors: bool,

    /// Set to true during function body checking when the body references `arguments`.
    /// Used in JS files to add an implicit rest parameter, allowing extra arguments.
    /// Save/restore pattern ensures correct handling across nested functions.
    pub js_body_uses_arguments: bool,

    /// Track which (node, symbol) pairs have already emitted TS2454 errors
    /// to avoid duplicate errors when the same usage is checked multiple times.
    /// Key: (`node_position`, `symbol_id`)
    pub emitted_ts2454_errors: CowCache<FxHashSet<(u32, SymbolId)>>,

    /// Track which (`interface_type`, `property_name`, `is_number_index`) combinations
    /// have already emitted TS2411 errors for merged interface declarations.
    /// When the same property appears in multiple declaration bodies (e.g., `1` in one
    /// body and `'1'` in another), we only report the error once.
    /// Key: (`interface_type_id.0`, `normalized_prop_name`, `is_number_index`)
    pub emitted_ts2411_for_iface_prop: FxHashSet<(u32, String, bool)>,

    /// Fuel counter for type resolution operations.
    /// Decremented on each type resolution to prevent timeout on pathological types.
    /// When exhausted, type resolution returns ERROR to prevent infinite loops.
    pub type_resolution_fuel: Cell<u32>,
    /// Node cache for class/method/interface type param `TypeId`s (no `DefId` registration).
    /// Prevents `fresh_type_param` from minting distinct ids across independent
    /// `push_type_parameters` calls; see `intern_type_param_for_decl` for invariants.
    pub type_param_node_cache: FxHashMap<(u32, tsz_solver::TypeParamInfo), tsz_solver::TypeId>,
}
