//! Checker Context
//!
//! Holds the shared state used throughout the type checking process.
//! This separates state from logic, allowing specialized checkers (expressions, statements)
//! to borrow the context mutably.
//!
//! Sub-modules:
//! - `constructors` - `CheckerContext` constructor methods
//! - `resolver` - `TypeResolver` trait implementation
//! - `def_mapping` - DefId migration helpers
//! - `compiler_options` - Compiler option accessors and solver config derivation
//! - `lib_queries` - Library/global type availability queries
//! - `module_entity` - Module entity resolution (`module_resolves_to_non_module_entity`)

mod compiler_options;
pub(crate) use compiler_options::is_declaration_file_name;
mod constructors;
mod core;
mod def_mapping;
mod lib_queries;
mod module_entity;
mod request_cache;
mod resolver;
pub(crate) mod speculation;
mod strict_mode;
pub mod typing_request;
pub use request_cache::{RequestCacheCounters, RequestCacheKey};
pub use typing_request::{ContextualOrigin, FlowIntent, TypingRequest};

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use tsz_common::interner::Atom;

use crate::control_flow::FlowGraph;
use crate::diagnostics::Diagnostic;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::def::{DefId, DefinitionStore};
use tsz_solver::{QueryDatabase, TypeEnvironment, TypeId};

// Re-export CheckerOptions and ScriptTarget from tsz-common
use tsz_binder::{BinderState, ModuleAugmentation};
pub use tsz_common::checker_options::CheckerOptions;
pub use tsz_common::common::ScriptTarget;
use tsz_parser::parser::node::NodeArena;

/// Maximum depth for nested `get_type_of_symbol` calls before giving up.
///
/// Prevents stack overflow when resolving deeply recursive or circular
/// symbol references (e.g., mutually referencing type aliases, deeply
/// nested namespace exports). Matches `MAX_INSTANTIATION_DEPTH` (50).
pub(crate) const MAX_SYMBOL_RESOLUTION_DEPTH: u32 = 50;

type ResolvedModulePathMap = FxHashMap<(usize, String), usize>;
type ResolvedModuleErrorMap = FxHashMap<(usize, String), ResolutionError>;

/// Represents a failed module resolution with specific error details.
#[derive(Clone, Debug)]
pub struct ResolutionError {
    pub code: u32,
    pub message: String,
}

/// Pre-built global index of all declared/ambient module names across all binders.
///
/// Separates exact module names (O(1) `HashSet` lookup) from wildcard patterns
/// (small linear scan). Built once in `set_all_binders` and shared via `Arc`.
#[derive(Debug, Default)]
pub struct GlobalDeclaredModules {
    /// Exact module names from `declared_modules`, `shorthand_ambient_modules`,
    /// and `module_exports` keys (normalized: quotes stripped).
    pub exact: FxHashSet<String>,
    /// Wildcard patterns (e.g., `*.css`, `*/theme`) that require glob matching.
    pub patterns: Vec<String>,
}

impl GlobalDeclaredModules {
    /// Build from pre-computed skeleton sets.
    ///
    /// `skeleton_exact` and `skeleton_patterns` come from
    /// `SkeletonIndex::build_declared_module_sets()`. The patterns must already
    /// be sorted and deduplicated (the skeleton builder guarantees this).
    #[must_use]
    pub const fn from_skeleton(exact: FxHashSet<String>, patterns: Vec<String>) -> Self {
        Self { exact, patterns }
    }
}

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

/// Deferred implicit-any diagnostic state for a variable declaration.
#[derive(Clone, Copy, Debug)]
pub struct PendingImplicitAnyVar {
    /// Declaration name node used for the TS7034 anchor.
    pub name_node: NodeIndex,
    /// Which deferred implicit-any behavior applies to this declaration.
    pub kind: PendingImplicitAnyKind,
}

/// Persistent cache for type checking results across LSP queries.
/// This cache survives between LSP requests but is invalidated when the file changes.
#[derive(Clone, Debug)]
pub struct TypeCache {
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached instance types for class symbols (for TYPE position).
    /// Distinguishes from `symbol_types` which holds constructor types for VALUE position.
    pub symbol_instance_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Maps `DefIds` to `SymbolIds` for declaration emit usage analysis.
    /// Populated by `CheckerContext` during type checking, consumed by `UsageAnalyzer`.
    pub def_to_symbol: FxHashMap<tsz_solver::DefId, SymbolId>,

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
    pub class_instance_type_cache: FxHashMap<NodeIndex, TypeId>,

    /// Forward cache: class declaration `NodeIndex` -> computed constructor `TypeId`.
    /// Avoids recomputing constructor shape/inheritance on repeated class queries.
    pub class_constructor_type_cache: FxHashMap<NodeIndex, TypeId>,

    /// Set of import specifier nodes that should be elided from JavaScript output.
    /// These are imports that reference type-only declarations (interfaces, type aliases).
    pub type_only_nodes: FxHashSet<NodeIndex>,

    /// Maps namespace `TypeIds` to their module display name.
    /// Used to display namespace types as `typeof import("module")` in diagnostics.
    /// Persists across file checks because `NS_CONSTRUCT` may run for one file
    /// while TS2339 is emitted from another.
    pub namespace_module_names: FxHashMap<TypeId, String>,
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

    /// Count of spelling suggestions (TS2552) emitted to limit output size.
    pub spelling_suggestions_emitted: u32,

    // --- Caches ---
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached instance types for class symbols (for TYPE position).
    /// Distinguishes from `symbol_types` which holds constructor types for VALUE position.
    pub symbol_instance_types: FxHashMap<SymbolId, TypeId>,

    /// Cached namespace object types for enums (for `typeof Enum` / `keyof typeof Enum`).
    /// Maps enum `SymbolId` → namespace object `TypeId` with member names as properties.
    pub enum_namespace_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

    /// Cache for `resolve_lib_type_by_name` results.
    /// Keyed by type name and stores both hits (`Some(TypeId)`) and misses (`None`).
    pub lib_type_resolution_cache: FxHashMap<String, Option<TypeId>>,

    /// Cache for lib delegation results in `delegate_cross_arena_symbol_resolution`.
    /// Keyed by SymbolId, stores the resolved TypeId. Prevents redundant child
    /// checker creation for the same lib symbol, which is the primary cause of
    /// hangs in multi-file tests with complex type libraries (react.d.ts has
    /// hundreds of DOM types that each trigger delegation).
    pub lib_delegation_cache: FxHashMap<SymbolId, TypeId>,

    /// Shared lib type resolution cache across parallel file checks.
    /// Uses `DashMap` for thread-safe concurrent access.
    pub shared_lib_type_cache: Option<Arc<dashmap::DashMap<String, Option<TypeId>>>>,

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
    pub lib_heritage_in_progress: FxHashSet<String>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Request-aware cache for audited non-empty request paths only.
    pub request_node_types: FxHashMap<(u32, RequestCacheKey), TypeId>,

    /// Internal counters for request-aware cache usage and cache-clear churn.
    pub request_cache_counters: RequestCacheCounters,

    /// Cached type environment for resolving Ref types during assignability checks.
    pub type_environment: Rc<RefCell<TypeEnvironment>>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

    /// Recursion guard for `evaluate_type_with_resolution`.
    /// Prevents infinite mutual recursion through
    /// `evaluate_type_with_resolution → prune_impossible_object_union_members_with_env
    /// → object_member_has_impossible_required_property_with_env → evaluate_type_with_resolution`
    /// on recursive type aliases.
    pub type_resolution_visiting: FxHashSet<TypeId>,
    /// Reentrancy guard for `prune_impossible_object_union_members_with_env`.
    /// Prevents infinite mutual recursion: evaluate → prune → evaluate → prune.
    pub pruning_union_members: bool,

    /// Recursion guard for `resolve_jsdoc_typedef_type`.
    /// Prevents infinite recursion when a JSDoc `@typedef` references itself
    /// (e.g., `@typedef {... | Json[]} Json`).
    pub jsdoc_typedef_resolving: RefCell<rustc_hash::FxHashSet<String>>,

    /// Cache for control flow analysis results.
    /// Key: (`FlowNodeId`, `SymbolId`, `InitialTypeId`) -> `NarrowedTypeId`
    /// Prevents re-traversing the flow graph for the same symbol/flow combination.
    /// Fixes performance regression on binaryArithmeticControlFlowGraphNotTooLarge.ts
    /// where each operand in a + b + c was triggering fresh graph traversals.
    pub flow_analysis_cache:
        RefCell<FxHashMap<(tsz_binder::FlowNodeId, tsz_binder::SymbolId, TypeId), TypeId>>,

    /// Reusable buffers for flow analysis to avoid frequent heap allocations in `check_flow`.
    pub flow_worklist: RefCell<VecDeque<(tsz_binder::FlowNodeId, TypeId)>>,
    pub flow_in_worklist: RefCell<FxHashSet<tsz_binder::FlowNodeId>>,
    pub flow_visited: RefCell<FxHashSet<tsz_binder::FlowNodeId>>,
    pub flow_results: RefCell<FxHashMap<tsz_binder::FlowNodeId, TypeId>>,

    /// Shared cache for narrowing operations (type resolution, property lookup).
    /// Reused across flow analysis passes to prevent O(N^2) behavior in CFA chains.
    pub narrowing_cache: tsz_solver::NarrowingCache,

    /// Cache for `is_narrowable_identifier` results.
    /// This is pure (depends only on AST structure), so it never needs invalidation.
    /// Avoids 4-5 binder/arena lookups per call on the hot cached-node path.
    pub narrowable_identifier_cache: RefCell<FxHashMap<u32, bool>>,

    /// Cache for switch-reference relevance checks.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_switch_reference_cache: RefCell<FxHashMap<(u32, u32), bool>>,

    /// Cache numeric atom conversions during flow analysis.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_numeric_atom_cache: RefCell<FxHashMap<u64, Atom>>,

    /// Shared reference-equivalence cache used by flow narrowing.
    /// Key: (`node_a`, `node_b`) -> whether they reference the same symbol/property chain.
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub flow_reference_match_cache: RefCell<FxHashMap<(u32, u32), bool>>,

    /// Cache for last assignment position per symbol, used by closure narrowing.
    /// Key: `SymbolId` -> last assignment byte position (0 = never reassigned).
    /// Reused across `FlowAnalyzer` instances within a single file check.
    pub symbol_last_assignment_pos: RefCell<FxHashMap<SymbolId, u32>>,

    /// Stable flow cache: maps `(SymbolId, DeclaredTypeId)` to the last `FlowNodeId`
    /// where flow analysis confirmed no narrowing (returned the declared type unchanged).
    /// When a new flow node for the same symbol can reach the confirmed node via a
    /// straight-line chain (no `CONDITION/ASSIGNMENT/BRANCH_LABEL` nodes), flow analysis
    /// is skipped entirely, returning the declared type directly.
    /// This eliminates O(N) flow cache misses for N sequential accesses to the same
    /// identifier (e.g., 34 references to `options` in sequential statements).
    pub symbol_flow_confirmed: RefCell<FxHashMap<(SymbolId, TypeId), tsz_binder::FlowNodeId>>,

    /// Instantiated type predicates from generic call resolutions.
    /// Keyed by call expression node index. Used by flow narrowing to get
    /// predicates with inferred type arguments applied (e.g., `T` -> `string`).
    pub call_type_predicates: crate::control_flow::CallPredicateMap,

    /// Nodes where TS2454 (used before assigned) was emitted.
    /// When TS2454 fires, `check_flow_usage` returns the declared type (un-narrowed).
    /// The second narrowing pass in `get_type_of_node` must NOT re-narrow these nodes,
    /// otherwise the declared type gets overridden with the narrowed type.
    pub daa_error_nodes: FxHashSet<u32>,

    /// Deferred TS2454 diagnostics that survive speculative rollback.
    /// `check_flow_usage` can run inside speculative call-checker contexts
    /// (generic inference, overload probing) that truncate diagnostics on
    /// rollback. To prevent TS2454 from being silently discarded, the error
    /// is buffered here and emitted at the end of `check_source_file`.
    pub deferred_ts2454_errors: Vec<(NodeIndex, SymbolId)>,

    /// Nodes where `check_flow_usage` already applied flow narrowing.
    /// The second narrowing pass in `get_type_of_node` must skip these to avoid
    /// double-narrowing (e.g., `any` → `string` → `string & Object`).
    pub flow_narrowed_nodes: FxHashSet<u32>,

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

    /// Maps `file_id` -> module specifier for import-qualified type display.
    /// When a type is defined in a module file, the formatter qualifies its name
    /// as `import("specifier").TypeName` to match tsc's behavior.
    /// Built from the arena's `source_files` during checker construction.
    pub module_specifiers: FxHashMap<u32, String>,

    /// Maps class instance `TypeIds` to their class declaration `NodeIndex`.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members (and thus no brand).
    /// Populated by `get_class_instance_type_inner` when creating class instance types.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration `NodeIndex` -> computed instance `TypeId`.
    /// Avoids recomputing the full class instance type on every member check.
    pub class_instance_type_cache: FxHashMap<NodeIndex, TypeId>,

    /// Forward cache: class declaration `NodeIndex` -> computed constructor `TypeId`.
    /// Avoids recomputing constructor inheritance checks in class-heavy programs.
    pub class_constructor_type_cache: FxHashMap<NodeIndex, TypeId>,

    /// Shared evaluation cache for `evaluate_type_with_env` results.
    /// Avoids re-evaluating the same `TypeId` through recursive mapped/conditional
    /// types on every call (e.g., `DeepPartial<Normalize<T>>` accessed 11k+ times
    /// in optional-chain-heavy benchmarks). Analogous to `node_types` for nodes.
    ///
    /// The cache also preserves whether evaluation exceeded the solver recursion
    /// limit so follow-up validation passes can still surface TS2589 from a cache hit.
    pub(crate) env_eval_cache: RefCell<FxHashMap<TypeId, EnvEvalCacheEntry>>,

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
    /// Positions of "real" syntax errors only (matching `is_real_syntax_error()`).
    /// Used for per-node TS2564 suppression — only real parse failures (not grammar
    /// checks like TS1030 "modifier already seen") suppress property initialization.
    pub real_syntax_error_positions: Vec<u32>,
    /// Positions of ALL parse errors (including non-suppressing ones like TS1359).
    /// Used for TS2456 suppression when a parse error falls within a type alias.
    pub all_parse_error_positions: Vec<u32>,

    /// Diagnostics produced during type checking.
    pub diagnostics: Vec<Diagnostic>,
    /// Set of already-emitted diagnostics (start, code) for deduplication.
    pub emitted_diagnostics: FxHashSet<(u32, u32)>,
    /// Callback return-type TS2322 diagnostics that were emitted during
    /// function body checking but may be pruned by arg collection filters.
    /// Stored separately so they can be restored after pruning and used to
    /// suppress the outer TS2345 argument mismatch.
    pub callback_return_type_errors: Vec<Diagnostic>,
    /// Set of modules that have already had TS2307 emitted (prevents duplicate emissions).
    pub modules_with_ts2307_emitted: FxHashSet<String>,

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
    /// Tracks module specifiers whose namespace types are currently being computed.
    /// Prevents infinite recursion when circular module imports eagerly resolve all exports
    /// (e.g. react's `prop-types` ↔ `react` cycle in react16.d.ts).
    pub module_namespace_resolution_set: FxHashSet<String>,
    /// Maps import `SymbolIds` to their `TYPE_ALIAS` body type, for imports that merge
    /// a type alias with a namespace re-export (e.g., `export type X = ...` + `export * as X from ...`).
    /// Populated during named import resolution in `compute_type_of_symbol`.
    /// Consumed by `type_reference_symbol_type` to return the type alias body in type contexts.
    pub import_type_alias_types: FxHashMap<SymbolId, TypeId>,
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: FxHashSet<SymbolId>,
    /// O(1) lookup set for class constructor type resolution to avoid recursion.
    pub class_constructor_resolution_set: FxHashSet<SymbolId>,
    /// Classes/interfaces with circular inheritance (TS2506/TS2310). Used to
    /// fix the return type of `new` on circular generic classes: tsc returns
    /// `C<unknown>` instead of the raw `C<T>`.
    pub circular_class_symbols: FxHashSet<SymbolId>,
    /// Deferred implicit-any candidates keyed by variable symbol.
    /// Bare implicit-any declarations stay capture-only, while evolving empty
    /// arrays also report on unsafe same-scope reads.
    pub pending_implicit_any_vars: FxHashMap<SymbolId, PendingImplicitAnyVar>,
    /// Closure/function-expression sites whose return expressions read a variable
    /// symbol currently being resolved. Used to centralize TS7022/TS7023/TS7024
    /// emission and suppress downstream relation noise from the circularity.
    pub pending_circular_return_sites: FxHashMap<SymbolId, Vec<NodeIndex>>,
    /// Extra tracking depth for method/accessor return-site circularity when a
    /// construct consults those bodies immediately during type computation
    /// (currently the `for...of` iterator protocol path).
    pub non_closure_circular_return_tracking_depth: usize,
    /// Variables that have already had TS7034 emitted, keyed by the deferred
    /// implicit-any classification that triggered it.
    pub reported_implicit_any_vars: FxHashMap<SymbolId, PendingImplicitAnyKind>,

    /// Inheritance graph tracking class/interface relationships
    pub inheritance_graph: tsz_solver::classes::inheritance::InheritanceGraph,

    /// Stack of nodes being resolved.
    pub node_resolution_stack: Vec<NodeIndex>,
    /// O(1) lookup set for node resolution stack.
    pub node_resolution_set: FxHashSet<NodeIndex>,

    /// Closures where implicit any (TS7006/TS7031) checks have already been performed.
    /// Prevents duplicate diagnostics when `get_type_of_function` is called multiple
    /// times for the same closure (e.g., once with contextual type during call
    /// resolution, then again without context during body checking).
    pub implicit_any_checked_closures: FxHashSet<NodeIndex>,
    /// Closures that have already been checked with a real contextual parameter type.
    /// Preserve this across cache clears so later context-free rechecks do not
    /// emit false TS7006/TS7031 diagnostics.
    pub implicit_any_contextual_closures: FxHashSet<NodeIndex>,

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

    /// Depth counter for conditional type `extends` clauses.
    /// Incremented when recursing into the `extends_type` of a conditional type,
    /// used to validate TS1338: `infer` only allowed in conditional extends.
    pub in_conditional_extends_depth: u32,

    /// Temporary scope for value parameters visible to `typeof` in return type annotations.
    /// Populated during signature processing so `typeof paramName` in return types
    /// can resolve to the parameter's type.
    pub typeof_param_scope: FxHashMap<String, TypeId>,

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

    /// Whether to skip flow narrowing when computing types.
    /// Used in assignment target type resolution to get declared types instead of narrowed types.
    /// When checking `foo[x] = 1` after `if (foo[x] === undefined)`, we need the declared type
    /// (e.g., `number | undefined`) not the narrowed type (e.g., `undefined`).
    pub skip_flow_narrowing: bool,

    /// Current depth of recursive type instantiation.
    pub instantiation_depth: Cell<u32>,

    /// Whether type instantiation depth was exceeded (for TS2589 emission).
    pub depth_exceeded: Cell<bool>,

    /// Explicit evaluation session state (replaces thread-local depth/fuel guards).
    /// Shared via `Rc` across parent/child contexts so counters survive cross-arena
    /// delegation without implicit global state.
    pub eval_session: Rc<tsz_solver::EvaluationSession>,

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
    pub generator_yield_operand_types: Vec<TypeId>,
    /// Whether TS7057 was emitted for any yield in the current generator.
    /// When true, TS7055 is suppressed (tsc emits one or the other, not both).
    pub generator_had_ts7057: bool,
    /// Stack of current `this` types for class member bodies.
    pub this_type_stack: Vec<TypeId>,

    /// Current enclosing class info.
    pub enclosing_class: Option<EnclosingClassInfo>,

    /// Stack of outer enclosing class `NodeIndex`es, from outermost to innermost.
    /// When entering a nested class, the outer class is pushed here.
    /// Used by protected member access checks (TS2446) to find the correct
    /// enclosing class in the inheritance hierarchy when code is inside nested classes.
    pub enclosing_class_chain: Vec<NodeIndex>,

    /// Type environment for symbol resolution with type parameters.
    /// Used by the evaluator to expand Application types.
    pub type_env: RefCell<TypeEnvironment>,

    // --- DefId Migration Infrastructure ---
    /// Storage for type definitions (interfaces, classes, type aliases).
    /// Part of the `DefId` migration to decouple Solver from Binder.
    pub definition_store: Arc<DefinitionStore>,

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

    /// Abstract constructor types (`TypeIds`) produced for abstract classes.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Protected constructor types (`TypeIds`) produced for protected constructors.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Private constructor types (`TypeIds`) produced for private constructors.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// Maps cross-file `SymbolIds` to their source file index.
    /// Populated by `resolve_cross_file_export/resolve_cross_file_namespace_exports`
    /// so `delegate_cross_arena_symbol_resolution` can find the correct arena.
    ///
    /// This is the local overlay for dynamically discovered mappings. For lookups,
    /// use `resolve_symbol_file_index()` which checks this overlay first, then
    /// falls back to the shared `global_symbol_file_index`.
    pub cross_file_symbol_targets: RefCell<FxHashMap<SymbolId, usize>>,

    /// Shared base map: `SymbolId` → owning file index (pre-built from `ProjectEnv`).
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
    pub global_file_locals_index: Option<Arc<FxHashMap<String, Vec<(usize, SymbolId)>>>>,

    /// Pre-built global index: (`module_specifier`, `export_name`) -> list of (`file_idx`, SymbolId).
    /// Constructed once in `set_all_binders` from all binders' `module_exports`.
    /// Eliminates O(N) scans in `resolve_import_from_ambient_module`.
    pub global_module_exports_index:
        Option<Arc<FxHashMap<String, FxHashMap<String, Vec<(usize, SymbolId)>>>>>,

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
    pub global_module_augmentations_index:
        Option<Arc<FxHashMap<String, Vec<(usize, ModuleAugmentation)>>>>,

    /// Pre-built global index: `module_specifier` -> Vec<(SymbolId, `file_idx`)>.
    /// Merges all binders' `augmentation_target_modules` (reverse map: symbol -> module)
    /// into a forward lookup: module -> symbols. Eliminates O(N) scans when finding
    /// augmentation symbols for a given module specifier (`interface_type.rs`).
    pub global_augmentation_targets_index: Option<Arc<FxHashMap<String, Vec<(SymbolId, usize)>>>>,

    /// Pre-built global index: module name -> list of binder indices that have that module
    /// in their `module_exports`. Eliminates O(N) binder scans when looking up which
    /// file(s) declared a given ambient module. Both raw and normalized (quote-stripped)
    /// forms of each module name are indexed.
    pub global_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>>,

    /// Pre-built arena-pointer → file-index map. Eliminates O(N) scans in
    /// `get_binder_for_arena` / `get_file_idx_for_arena` (13+ call sites).
    /// Key is `Arc::as_ptr(arena) as usize` for `Send`/`Sync` safety.
    pub global_arena_index: Option<Arc<FxHashMap<usize, usize>>>,

    /// Resolved module paths map: (`source_file_idx`, specifier) -> `target_file_idx`.
    /// Used by `get_type_of_symbol` to resolve imports to their target file and symbol.
    ///
    /// Key invariant: all specifier lookups should use
    /// `module_resolution::module_specifier_candidates` for canonical variants.
    pub resolved_module_paths: Option<Arc<ResolvedModulePathMap>>,

    /// Current file index in multi-file mode (index into `all_arenas/all_binders`).
    /// Used with `resolved_module_paths` to look up cross-file imports.
    pub current_file_idx: usize,

    /// Resolved module specifiers for this file (multi-file CLI mode).
    pub resolved_modules: Option<FxHashSet<String>>,

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
    pub lib_contexts: Vec<LibContext>,

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

    /// Closure depth - tracks nesting of function expressions, arrow functions, and method expressions.
    /// Used to apply Rule #42: CFA Invalidation in Closures.
    /// When > 0, mutable variables (let/var) lose narrowing in closures.
    pub inside_closure_depth: u32,

    /// When true, we're inside a const assertion (as const) and should preserve literal types.
    /// This prevents widening of literal types in object/array literals.
    pub in_const_assertion: bool,

    /// When true, preserve literal types instead of widening.
    /// Set during evaluation of compound expression branches (conditional `?:`,
    /// logical `||`/`&&`/`??`) so that `const x = cond ? "a" : "b"` infers
    /// `"a" | "b"` instead of `string`.
    pub preserve_literal_types: bool,

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

    /// Track whether current code path is syntactically unreachable.
    pub is_unreachable: bool,

    /// Track whether we have already reported an unreachable error in this block/scope.
    pub has_reported_unreachable: bool,

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
    pub emitted_ts2454_errors: FxHashSet<(u32, SymbolId)>,

    /// Fuel counter for type resolution operations.
    /// Decremented on each type resolution to prevent timeout on pathological types.
    /// When exhausted, type resolution returns ERROR to prevent infinite loops.
    pub type_resolution_fuel: Cell<u32>,
    // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
    // This fixes the "Zombie Freshness" bug by interning fresh vs non-fresh
    // object shapes distinctly.
}

/// Context for a lib file (arena + binder) for global type resolution.
#[derive(Clone, Debug)]
pub struct LibContext {
    /// The AST arena for this lib file.
    pub arena: Arc<NodeArena>,
    /// The binder state with symbols from this lib file.
    pub binder: Arc<BinderState>,
}

/// Project-wide shared environment for multi-file type checking.
///
/// Captures all the state that is identical across every per-file `CheckerContext`
/// in a project check run. Drivers (CLI, LSP) build one `ProjectEnv` after merge
/// and call [`ProjectEnv::apply_to`] on each checker instead of repeating 10+
/// setter calls per file.
///
/// This struct is `Clone`-cheap because every field is either `Arc`-wrapped or `Copy`.
#[derive(Clone)]
pub struct ProjectEnv {
    /// Lib file contexts for global type resolution.
    pub lib_contexts: Vec<LibContext>,
    /// All AST arenas for cross-file resolution (indexed by `file_idx`).
    pub all_arenas: Arc<Vec<Arc<NodeArena>>>,
    /// All binders for cross-file resolution (indexed by `file_idx`).
    pub all_binders: Arc<Vec<Arc<BinderState>>>,
    /// Pre-computed declared modules from skeleton index.
    pub skeleton_declared_modules: Option<Arc<GlobalDeclaredModules>>,
    /// Pre-computed expando index from skeleton index.
    pub skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>>,
    /// Pre-computed symbol-to-file ownership targets (legacy vec form).
    pub symbol_file_targets: Arc<Vec<(SymbolId, usize)>>,
    /// Pre-built O(1) index: `SymbolId` -> owning file index.
    ///
    /// Built once from `symbol_file_targets` by `build_global_symbol_file_index()`.
    /// Shared across all checkers via `Arc` — child checkers clone the `Arc`
    /// reference (O(1)) instead of cloning the entire `FxHashMap`.
    /// Read sites fall back to this base map when the local
    /// `cross_file_symbol_targets` overlay has no entry.
    pub global_symbol_file_index: Option<Arc<FxHashMap<SymbolId, usize>>>,
    /// Pre-computed global `file_locals` index: name -> Vec<(`file_idx`, SymbolId)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_file_locals_index: Option<Arc<FxHashMap<String, Vec<(usize, SymbolId)>>>>,
    /// Pre-computed global `module_exports` index: (specifier, `export_name`) -> Vec<(`file_idx`, SymbolId)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_exports_index:
        Option<Arc<FxHashMap<String, FxHashMap<String, Vec<(usize, SymbolId)>>>>>,
    /// Pre-computed global module augmentations index: specifier -> Vec<(`file_idx`, `ModuleAugmentation`)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_augmentations_index:
        Option<Arc<FxHashMap<String, Vec<(usize, ModuleAugmentation)>>>>,
    /// Pre-computed global augmentation targets index: specifier -> Vec<(SymbolId, `file_idx`)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_augmentation_targets_index: Option<Arc<FxHashMap<String, Vec<(SymbolId, usize)>>>>,
    /// Pre-computed global module binder index: module name -> Vec<`binder_idx`>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>>,
    /// Pre-computed arena-pointer → file-index map. O(1) arena→binder lookups.
    pub global_arena_index: Option<Arc<FxHashMap<usize, usize>>>,
    /// Resolved module paths: (`source_file_idx`, specifier) -> `target_file_idx`.
    pub resolved_module_paths: Arc<ResolvedModulePathMap>,
    /// Resolved module errors: (`source_file_idx`, specifier) -> error details.
    pub resolved_module_errors: Arc<ResolvedModuleErrorMap>,
    /// Per-file external module status.
    pub is_external_module_by_file: Arc<FxHashMap<String, bool>>,
    /// Per-file ESM/CJS determination.
    pub file_is_esm_map: Arc<FxHashMap<String, bool>>,
    /// Whether a @typescript/lib-dom replacement was loaded, and its window/self globals.
    pub typescript_dom_replacement_globals: (bool, bool, bool),
    /// Whether TS5107/TS5101 deprecation diagnostics are present.
    pub has_deprecation_diagnostics: bool,
    /// Skeleton fingerprint from the last `build_global_indices` call.
    ///
    /// When set, `build_global_indices_if_changed` can compare the new skeleton
    /// fingerprint against this value and skip the expensive O(N) binder scan
    /// when the project topology is unchanged.
    pub last_skeleton_fingerprint: Option<u64>,
}

impl Default for ProjectEnv {
    fn default() -> Self {
        Self {
            lib_contexts: vec![],
            all_arenas: Arc::new(vec![]),
            all_binders: Arc::new(vec![]),
            skeleton_declared_modules: None,
            skeleton_expando_index: None,
            symbol_file_targets: Arc::new(vec![]),
            global_symbol_file_index: None,
            global_file_locals_index: None,
            global_module_exports_index: None,
            global_module_augmentations_index: None,
            global_augmentation_targets_index: None,
            global_module_binder_index: None,
            global_arena_index: None,
            resolved_module_paths: Arc::new(FxHashMap::default()),
            resolved_module_errors: Arc::new(FxHashMap::default()),
            is_external_module_by_file: Arc::new(FxHashMap::default()),
            file_is_esm_map: Arc::new(FxHashMap::default()),
            typescript_dom_replacement_globals: (false, false, false),
            has_deprecation_diagnostics: false,
            last_skeleton_fingerprint: None,
        }
    }
}

impl ProjectEnv {
    /// Apply all project-level shared state to a checker context.
    ///
    /// This replaces the 10+ individual setter calls that drivers previously
    /// repeated at every checker creation site. The order of operations matches
    /// the original driver pattern: skeleton indices are set before `set_all_binders`
    /// so the binder scan can be skipped for `declared_modules` and expando.
    pub fn apply_to(&self, ctx: &mut CheckerContext<'_>) {
        if !self.lib_contexts.is_empty() {
            ctx.set_lib_contexts(self.lib_contexts.clone());
            ctx.set_actual_lib_file_count(self.lib_contexts.len());
        }
        ctx.set_typescript_dom_replacement_globals(
            self.typescript_dom_replacement_globals.0,
            self.typescript_dom_replacement_globals.1,
            self.typescript_dom_replacement_globals.2,
        );
        ctx.set_has_deprecation_diagnostics(self.has_deprecation_diagnostics);
        ctx.set_all_arenas(Arc::clone(&self.all_arenas));
        if let Some(ref dm) = self.skeleton_declared_modules {
            ctx.set_declared_modules_from_skeleton(Arc::clone(dm));
        }
        if let Some(ref ei) = self.skeleton_expando_index {
            ctx.set_expando_index_from_skeleton(Arc::clone(ei));
        }
        // Pre-install global indices before set_all_binders so it can skip
        // re-computing them. This avoids O(N) binder scans per checker.
        if let Some(ref idx) = self.global_file_locals_index {
            ctx.global_file_locals_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_exports_index {
            ctx.global_module_exports_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_augmentations_index {
            ctx.global_module_augmentations_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_augmentation_targets_index {
            ctx.global_augmentation_targets_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_binder_index {
            ctx.global_module_binder_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_arena_index {
            ctx.global_arena_index = Some(Arc::clone(idx));
        }
        ctx.set_all_binders(Arc::clone(&self.all_binders));
        // Pre-populate DefIds from all cross-file binders' semantic_defs.
        // This moves identity creation to apply_to time (deterministic, early)
        // rather than on-demand in get_or_create_def_id's O(N) repair path.
        ctx.pre_populate_def_ids_from_all_binders();
        // Resolve cross-batch heritage now that all DefIds from all binders
        // are registered. This wires up extends/implements at the DefId level.
        ctx.resolve_cross_batch_heritage();
        // Install the shared O(1) symbol→file index. When present, all base entries
        // are accessible via `resolve_symbol_file_index()`, so we skip the O(N) copy
        // into the local overlay. Only fall back to the O(N) copy when no global
        // index was built (e.g., in tests that don't call `build_global_symbol_file_index`).
        if let Some(ref idx) = self.global_symbol_file_index {
            ctx.global_symbol_file_index = Some(Arc::clone(idx));
        } else if !self.symbol_file_targets.is_empty() {
            let mut targets = ctx.cross_file_symbol_targets.borrow_mut();
            for &(sym_id, owner_idx) in self.symbol_file_targets.iter() {
                targets.insert(sym_id, owner_idx);
            }
        }
        ctx.set_resolved_module_paths(Arc::clone(&self.resolved_module_paths));
        ctx.set_resolved_module_errors(Arc::clone(&self.resolved_module_errors));
        ctx.is_external_module_by_file = Some(Arc::clone(&self.is_external_module_by_file));
        ctx.file_is_esm_map = Some(Arc::clone(&self.file_is_esm_map));
    }

    /// Build the 4 global binder indices from `all_binders`.
    ///
    /// This is the same computation that `set_all_binders` does, but factored out
    /// so drivers can compute it once and share via `Arc` across all checkers.
    /// When these fields are `Some`, `set_all_binders` skips re-computing them.
    pub fn build_global_indices(&mut self) {
        let mut file_locals_index: FxHashMap<String, Vec<(usize, SymbolId)>> = FxHashMap::default();
        let mut module_exports_index: FxHashMap<String, FxHashMap<String, Vec<(usize, SymbolId)>>> =
            FxHashMap::default();
        let mut module_augs_index: FxHashMap<String, Vec<(usize, ModuleAugmentation)>> =
            FxHashMap::default();
        let mut aug_targets_index: FxHashMap<String, Vec<(SymbolId, usize)>> = FxHashMap::default();
        let mut module_binder_index: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        // Also build declared_modules if not already from skeleton.
        let mut declared_modules = if self.skeleton_declared_modules.is_some() {
            None
        } else {
            Some(GlobalDeclaredModules::default())
        };

        for (file_idx, binder) in self.all_binders.iter().enumerate() {
            for (name, &sym_id) in binder.file_locals.iter() {
                file_locals_index
                    .entry(name.to_string())
                    .or_default()
                    .push((file_idx, sym_id));
            }
            for (module_spec, exports) in binder.module_exports.iter() {
                // Build module_binder_index: module_spec -> [binder_idx]
                module_binder_index
                    .entry(module_spec.clone())
                    .or_default()
                    .push(file_idx);
                let normalized = module_spec.trim_matches('"').trim_matches('\'');
                if normalized != module_spec {
                    module_binder_index
                        .entry(normalized.to_string())
                        .or_default()
                        .push(file_idx);
                }
                for (export_name, &sym_id) in exports.iter() {
                    module_exports_index
                        .entry(module_spec.clone())
                        .or_default()
                        .entry(export_name.to_string())
                        .or_default()
                        .push((file_idx, sym_id));
                }
                if let Some(ref mut dm) = declared_modules {
                    let normalized = module_spec.trim_matches('"').trim_matches('\'');
                    if normalized.contains('*') {
                        dm.patterns.push(normalized.to_string());
                    } else {
                        dm.exact.insert(normalized.to_string());
                    }
                }
            }
            if let Some(ref mut dm) = declared_modules {
                for name in binder
                    .declared_modules
                    .iter()
                    .chain(binder.shorthand_ambient_modules.iter())
                {
                    let normalized = name.trim_matches('"').trim_matches('\'');
                    if normalized.contains('*') {
                        dm.patterns.push(normalized.to_string());
                    } else {
                        dm.exact.insert(normalized.to_string());
                    }
                }
            }
            for (module_spec, augmentations) in binder.module_augmentations.iter() {
                module_augs_index
                    .entry(module_spec.clone())
                    .or_default()
                    .extend(augmentations.iter().map(|aug| (file_idx, aug.clone())));
            }
            for (&sym_id, module_spec) in binder.augmentation_target_modules.iter() {
                aug_targets_index
                    .entry(module_spec.clone())
                    .or_default()
                    .push((sym_id, file_idx));
            }
        }

        // Build expando index if not already from skeleton.
        if self.skeleton_expando_index.is_none() {
            let mut expando_index: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
            for binder in self.all_binders.iter() {
                for (obj_key, props) in binder.expando_properties.iter() {
                    expando_index
                        .entry(obj_key.clone())
                        .or_default()
                        .extend(props.iter().cloned());
                }
            }
            self.skeleton_expando_index = Some(Arc::new(expando_index));
        }

        if let Some(mut dm) = declared_modules {
            dm.patterns.sort();
            dm.patterns.dedup();
            self.skeleton_declared_modules = Some(Arc::new(dm));
        }

        self.global_file_locals_index = Some(Arc::new(file_locals_index));
        self.global_module_exports_index = Some(Arc::new(module_exports_index));
        self.global_module_augmentations_index = Some(Arc::new(module_augs_index));
        self.global_augmentation_targets_index = Some(Arc::new(aug_targets_index));
        self.global_module_binder_index = Some(Arc::new(module_binder_index));

        // Build arena-pointer → file-index map
        let mut arena_idx: FxHashMap<usize, usize> = FxHashMap::default();
        for (file_idx, arena) in self.all_arenas.iter().enumerate() {
            arena_idx.insert(Arc::as_ptr(arena) as usize, file_idx);
        }
        self.global_arena_index = Some(Arc::new(arena_idx));
    }

    /// Build the shared `SymbolId` → file-index map from `symbol_file_targets`.
    ///
    /// Call this once after populating `symbol_file_targets`. The resulting
    /// `Arc<FxHashMap>` is shared (O(1) clone) across all checkers, eliminating
    /// the per-checker O(N) copy into `cross_file_symbol_targets`.
    pub fn build_global_symbol_file_index(&mut self) {
        let mut map: FxHashMap<SymbolId, usize> =
            FxHashMap::with_capacity_and_hasher(self.symbol_file_targets.len(), Default::default());
        for &(sym_id, file_idx) in self.symbol_file_targets.iter() {
            map.insert(sym_id, file_idx);
        }
        self.global_symbol_file_index = Some(Arc::new(map));
    }

    /// Build global indices only when the skeleton fingerprint has changed.
    ///
    /// Compares `new_fingerprint` against `self.last_skeleton_fingerprint`.
    /// If they match, the global indices are already valid and the expensive
    /// O(N) binder scan is skipped entirely. If they differ (or this is the
    /// first build), delegates to `build_global_indices` and stores the new
    /// fingerprint for future comparisons.
    ///
    /// Returns `true` if indices were rebuilt, `false` if cached.
    pub fn build_global_indices_if_changed(&mut self, new_fingerprint: u64) -> bool {
        if self.last_skeleton_fingerprint == Some(new_fingerprint) {
            // All global indices (name-based + arena) + skeleton indices are still valid.
            return false;
        }
        self.build_global_indices();
        self.last_skeleton_fingerprint = Some(new_fingerprint);
        true
    }
}
