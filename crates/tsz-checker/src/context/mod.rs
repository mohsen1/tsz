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
pub use compiler_options::is_declaration_file_name;
mod constructors;
mod core;
mod def_mapping;
mod lib_queries;
mod module_entity;
mod resolver;
mod strict_mode;

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
use tsz_binder::BinderState;
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
    pub name: String,
    /// Whether the label is on an iteration statement (for continue validation).
    /// Only iteration labels can be targets of continue statements.
    pub is_iteration: bool,
    /// The function depth when this label was defined.
    /// Used to detect if a jump crosses a function boundary.
    pub function_depth: u32,
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

/// Info about a symbol that came from destructuring a union type.
/// Info about a symbol that came from destructuring a union type.
/// Used for correlated discriminant narrowing: when `const { data, isSuccess } = getResult()`,
/// narrowing `isSuccess` should also narrow `data`.
#[derive(Clone, Debug)]
pub struct DestructuredBindingInfo {
    /// The source type of the entire destructured expression (the union)
    pub source_type: TypeId,
    /// The property name that this symbol corresponds to (for object patterns)
    pub property_name: String,
    /// The element index for array/tuple patterns (`u32::MAX` if object pattern)
    pub element_index: u32,
    /// The binding group ID — all symbols from the same destructuring share this
    pub group_id: u32,
    /// Whether this is a const binding (only const bindings support correlated narrowing)
    pub is_const: bool,
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

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

    /// Cache for `resolve_lib_type_by_name` results.
    /// Keyed by type name and stores both hits (`Some(TypeId)`) and misses (`None`).
    pub lib_type_resolution_cache: FxHashMap<String, Option<TypeId>>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Cached type environment for resolving Ref types during assignability checks.
    pub type_environment: Rc<RefCell<TypeEnvironment>>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

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

    /// Instantiated type predicates from generic call resolutions.
    /// Keyed by call expression node index. Used by flow narrowing to get
    /// predicates with inferred type arguments applied (e.g., `T` -> `string`).
    pub call_type_predicates: crate::control_flow::CallPredicateMap,

    /// Nodes where TS2454 (used before assigned) was emitted.
    /// When TS2454 fires, `check_flow_usage` returns the declared type (un-narrowed).
    /// The second narrowing pass in `get_type_of_node` must NOT re-narrow these nodes,
    /// otherwise the declared type gets overridden with the narrowed type.
    pub daa_error_nodes: FxHashSet<u32>,

    /// Nodes where `check_flow_usage` already applied flow narrowing.
    /// The second narrowing pass in `get_type_of_node` must skip these to avoid
    /// double-narrowing (e.g., `any` → `string` → `string & Object`).
    pub flow_narrowed_nodes: FxHashSet<u32>,

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
    pub env_eval_cache: RefCell<FxHashMap<TypeId, TypeId>>,

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

    // --- Destructured Binding Tracking ---
    /// Maps destructured const binding symbols to their source union type info.
    /// Used for correlated discriminant narrowing (TS 4.6+ feature).
    pub destructured_bindings: FxHashMap<SymbolId, DestructuredBindingInfo>,
    /// Counter for generating unique binding group IDs.
    pub next_binding_group_id: u32,
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

    /// Diagnostics produced during type checking.
    pub diagnostics: Vec<Diagnostic>,
    /// Set of already-emitted diagnostics (start, code) for deduplication.
    pub emitted_diagnostics: FxHashSet<(u32, u32)>,
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
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: FxHashSet<SymbolId>,
    /// O(1) lookup set for class constructor type resolution to avoid recursion.
    pub class_constructor_resolution_set: FxHashSet<SymbolId>,
    /// Deferred TS7034 candidates: non-ambient variables with no annotation, no init, and type ANY.
    /// Maps symbol ID → declaration name node. Consumed when a capture is detected.
    pub pending_implicit_any_vars: FxHashMap<SymbolId, NodeIndex>,
    /// Variables that have already had TS7034 emitted.
    /// Used to emit TS7005 on subsequent usages.
    pub reported_implicit_any_vars: FxHashSet<SymbolId>,

    /// Inheritance graph tracking class/interface relationships
    pub inheritance_graph: tsz_solver::classes::inheritance::InheritanceGraph,

    /// Stack of nodes being resolved.
    pub node_resolution_stack: Vec<NodeIndex>,
    /// O(1) lookup set for node resolution stack.
    pub node_resolution_set: FxHashSet<NodeIndex>,

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

    /// Temporarily holds information about children of the current JSX element
    /// being checked. Set in dispatch.rs before calling `get_type_of_jsx_opening_element`,
    /// consumed in `check_jsx_attributes_against_props` for children validation.
    /// Contains (`child_count`, `has_text_child`, `synthesized_children_type`, `text_child_indices`).
    /// - `child_count`: number of children in the JSX body
    /// - `has_text_child`: whether any `JsxText` children exist
    /// - `synthesized_children_type`: the type to use as the `children` prop value
    /// - `text_child_indices`: node indices of `JsxText` children (for TS2747 location reporting)
    pub jsx_children_info: Option<(usize, bool, TypeId, Vec<NodeIndex>)>,

    /// The callable type of the current call expression being checked.
    /// Set before `collect_call_argument_types_with_context` so spread-handling
    /// code can query rest parameter positions via `ContextualTypeContext`.
    pub current_callable_type: Option<TypeId>,

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
    pub instantiation_depth: RefCell<u32>,

    /// Whether type instantiation depth was exceeded (for TS2589 emission).
    pub depth_exceeded: RefCell<bool>,

    /// General recursion depth counter for type checking.
    /// Prevents stack overflow by bailing out when depth exceeds the limit.
    pub recursion_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Current depth of call expression resolution.
    pub call_depth: RefCell<tsz_solver::recursion::DepthCounter>,

    /// Stack of expected return types for functions.
    pub return_type_stack: Vec<TypeId>,
    /// Stack of contextual yield types for generator functions.
    /// Used to contextually type yield expressions (prevents false TS7006).
    pub yield_type_stack: Vec<Option<TypeId>>,
    /// Collected yield operand types during body check for unannotated generators.
    /// After body check, the union determines the inferred yield type for TS7055/TS7025 vs TS7057.
    pub generator_yield_operand_types: Vec<TypeId>,
    /// Stack of current `this` types for class member bodies.
    pub this_type_stack: Vec<TypeId>,

    /// Current enclosing class info.
    pub enclosing_class: Option<EnclosingClassInfo>,

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

    /// Abstract constructor types (`TypeIds`) produced for abstract classes.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Protected constructor types (`TypeIds`) produced for protected constructors.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Private constructor types (`TypeIds`) produced for private constructors.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// Maps cross-file `SymbolIds` to their source file index.
    /// Populated by `resolve_cross_file_export/resolve_cross_file_namespace_exports`
    /// so `delegate_cross_arena_symbol_resolution` can find the correct arena.
    pub cross_file_symbol_targets: RefCell<FxHashMap<SymbolId, usize>>,

    /// All arenas for cross-file resolution (indexed by `file_idx` from `Symbol.decl_file_idx`).
    /// Set during multi-file type checking to allow resolving declarations across files.
    pub all_arenas: Option<Arc<Vec<Arc<NodeArena>>>>,

    /// All binders for cross-file resolution (indexed by `file_idx`).
    /// Enables looking up exported symbols from other files during import resolution.
    pub all_binders: Option<Arc<Vec<Arc<BinderState>>>>,

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
    pub label_stack: Vec<LabelInfo>,

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
    pub type_resolution_fuel: RefCell<u32>,
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
