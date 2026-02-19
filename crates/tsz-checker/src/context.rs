//! Checker Context
//!
//! Holds the shared state used throughout the type checking process.
//! This separates state from logic, allowing specialized checkers (expressions, statements)
//! to borrow the context mutably.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use tsz_common::interner::Atom;

use crate::control_flow::FlowGraph;
use crate::diagnostics::Diagnostic;
use crate::module_resolution::module_specifier_candidates;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::def::{DefId, DefinitionStore};
use tsz_solver::{QueryDatabase, TypeEnvironment, TypeId, judge::JudgeConfig};

// Re-export CheckerOptions and ScriptTarget from tsz-common
use tsz_binder::BinderState;
pub use tsz_common::checker_options::CheckerOptions;
pub use tsz_common::common::ScriptTarget;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

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
    /// Whether we're in a static method context.
    pub in_static_method: bool,
    /// Whether any `super()` call appeared while checking the current constructor body.
    pub has_super_call_in_current_constructor: bool,
    /// Cached instance `this` type for members of this class.
    pub cached_instance_this_type: Option<TypeId>,
    /// Names of the class's own type parameters (for TS2302 checking in static members).
    pub type_param_names: Vec<String>,
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

    /// Set of import specifier nodes that should be elided from JavaScript output.
    /// These are imports that reference type-only declarations (interfaces, type aliases).
    pub type_only_nodes: FxHashSet<NodeIndex>,
}

impl TypeCache {
    /// Invalidate cached symbol types that depend on the provided roots.
    /// Returns the number of affected symbols.
    pub fn invalidate_symbols(&mut self, roots: &[SymbolId]) -> usize {
        if roots.is_empty() {
            return 0;
        }

        let mut reverse: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (symbol, deps) in &self.symbol_dependencies {
            for dep in deps {
                reverse.entry(*dep).or_default().push(*symbol);
            }
        }

        let mut affected: FxHashSet<SymbolId> = FxHashSet::default();
        let mut pending = VecDeque::new();
        for &root in roots {
            if affected.insert(root) {
                pending.push_back(root);
            }
        }

        while let Some(sym_id) = pending.pop_front() {
            if let Some(dependents) = reverse.get(&sym_id) {
                for &dependent in dependents {
                    if affected.insert(dependent) {
                        pending.push_back(dependent);
                    }
                }
            }
        }

        for sym_id in &affected {
            self.symbol_types.remove(sym_id);
            self.symbol_instance_types.remove(sym_id);
            self.symbol_dependencies.remove(sym_id);
        }
        self.node_types.clear();
        affected.len()
    }

    /// Merge another `TypeCache` into this one.
    /// Used to accumulate type information from multiple file checks for declaration emit.
    pub fn merge(&mut self, other: Self) {
        self.symbol_types.extend(other.symbol_types);
        self.symbol_instance_types
            .extend(other.symbol_instance_types);
        self.node_types.extend(other.node_types);

        // Merge symbol dependencies sets
        for (sym, deps) in other.symbol_dependencies {
            self.symbol_dependencies
                .entry(sym)
                .or_default()
                .extend(deps);
        }

        // Merge def_to_symbol mapping
        self.def_to_symbol.extend(other.def_to_symbol);
    }
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

    // --- Caches ---
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached instance types for class symbols (for TYPE position).
    /// Distinguishes from `symbol_types` which holds constructor types for VALUE position.
    pub symbol_instance_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

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

    /// `TypeIds` whose application/lazy symbol references are fully resolved in `type_env`.
    /// This avoids repeated deep traversals in assignability hot paths.
    pub application_symbols_resolved: FxHashSet<TypeId>,

    /// Recursion guard for application symbol resolution traversal.
    pub application_symbols_resolution_set: FxHashSet<TypeId>,

    /// Maps class instance `TypeIds` to their class declaration `NodeIndex`.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members (and thus no brand).
    /// Populated by `get_class_instance_type_inner` when creating class instance types.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration `NodeIndex` -> computed instance `TypeId`.
    /// Avoids recomputing the full class instance type on every member check.
    pub class_instance_type_cache: FxHashMap<NodeIndex, TypeId>,

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
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: FxHashSet<SymbolId>,
    /// O(1) lookup set for class constructor type resolution to avoid recursion.
    pub class_constructor_resolution_set: FxHashSet<SymbolId>,
    /// Deferred TS7034 candidates: non-ambient variables with no annotation, no init, and type ANY.
    /// Maps symbol ID → declaration name node. Consumed when a capture is detected.
    pub pending_implicit_any_vars: FxHashMap<SymbolId, NodeIndex>,

    /// Inheritance graph tracking class/interface relationships
    pub inheritance_graph: tsz_solver::inheritance::InheritanceGraph,

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

    /// Contextual type for expression being checked.
    pub contextual_type: Option<TypeId>,

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

    /// Whether type resolution fuel was exhausted (for timeout detection).
    pub fuel_exhausted: RefCell<bool>,
    // NOTE: Freshness is now tracked on the TypeId via ObjectFlags.
    // This fixes the "Zombie Freshness" bug by interning fresh vs non-fresh
    // object shapes distinctly.
}

/// Context for a lib file (arena + binder) for global type resolution.
#[derive(Clone)]
pub struct LibContext {
    /// The AST arena for this lib file.
    pub arena: Arc<NodeArena>,
    /// The binder state with symbols from this lib file.
    pub binder: Arc<BinderState>,
}

impl<'a> CheckerContext<'a> {
    /// Set lib contexts for global type resolution.
    /// Note: `lib_contexts` may include both actual lib files AND user files for cross-file
    /// resolution. Use `set_actual_lib_file_count()` to track how many are actual lib files.
    pub fn set_lib_contexts(&mut self, lib_contexts: Vec<LibContext>) {
        self.lib_contexts = lib_contexts;
    }

    /// Set the count of actual lib files loaded (not including user files).
    /// This is used by `has_lib_loaded()` to correctly determine if standard library is available.
    pub const fn set_actual_lib_file_count(&mut self, count: usize) {
        self.actual_lib_file_count = count;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Arc<Vec<Arc<NodeArena>>>) {
        self.all_arenas = Some(arenas);
    }

    /// Set all binders for cross-file resolution.
    pub fn set_all_binders(&mut self, binders: Arc<Vec<Arc<BinderState>>>) {
        self.all_binders = Some(binders);
    }

    /// Set resolved module paths map for cross-file import resolution.
    pub fn set_resolved_module_paths(&mut self, paths: Arc<FxHashMap<(usize, String), usize>>) {
        self.resolved_module_paths = Some(paths);
    }

    /// Set resolved module specifiers (module names that exist in the project).
    /// Used to suppress TS2307 errors for known modules.
    pub fn set_resolved_modules(&mut self, modules: FxHashSet<String>) {
        self.resolved_modules = Some(modules);
    }

    /// Set resolved module errors map for cross-file import resolution.
    /// Populated by the driver when `ModuleResolver` returns specific errors (TS2834, TS2835, TS2792, etc.).
    pub fn set_resolved_module_errors(
        &mut self,
        errors: Arc<FxHashMap<(usize, String), ResolutionError>>,
    ) {
        self.resolved_module_errors = Some(errors);
    }

    /// Get the resolution error for a specifier, if any.
    /// Returns the specific error (TS2834, TS2835, TS2792, etc.) if the module resolution failed with a known error.
    pub fn get_resolution_error(&self, specifier: &str) -> Option<&ResolutionError> {
        let errors = self.resolved_module_errors.as_ref()?;

        for candidate in module_specifier_candidates(specifier) {
            if let Some(error) = errors.get(&(self.current_file_idx, candidate)) {
                return Some(error);
            }
        }
        None
    }

    /// Set the current file index.
    pub const fn set_current_file_idx(&mut self, idx: usize) {
        self.current_file_idx = idx;
    }

    /// Get the arena for a specific file index.
    /// Returns the current arena if `file_idx` is `u32::MAX` (single-file mode).
    pub fn get_arena_for_file(&self, file_idx: u32) -> &NodeArena {
        if file_idx == u32::MAX {
            return self.arena;
        }
        if let Some(arenas) = self.all_arenas.as_ref()
            && let Some(arena) = arenas.get(file_idx as usize)
        {
            return arena.as_ref();
        }
        self.arena
    }

    /// Get the binder for a specific file index.
    /// Returns None if `file_idx` is out of bounds or `all_binders` is not set.
    pub fn get_binder_for_file(&self, file_idx: usize) -> Option<&BinderState> {
        self.all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .map(Arc::as_ref)
    }

    /// Resolve an import specifier to its target file index.
    /// Uses the `resolved_module_paths` map populated by the driver.
    /// Returns None if the import cannot be resolved (e.g., external module).
    pub fn resolve_import_target(&self, specifier: &str) -> Option<usize> {
        self.resolve_import_target_from_file(self.current_file_idx, specifier)
    }

    /// Resolve an import specifier from a specific file to its target file index.
    /// Like `resolve_import_target` but for any source file, not just the current one.
    pub fn resolve_import_target_from_file(
        &self,
        source_file_idx: usize,
        specifier: &str,
    ) -> Option<usize> {
        let paths = self.resolved_module_paths.as_ref()?;
        for candidate in module_specifier_candidates(specifier) {
            if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                return Some(*target_idx);
            }
        }
        None
    }

    /// Returns true if an augmentation target resolves to an `export =` value without
    /// namespace/module shape (TS2671/TS2649 cases).
    pub fn module_resolves_to_non_module_entity(&self, module_specifier: &str) -> bool {
        let candidates = module_specifier_candidates(module_specifier);

        let lookup_cached = |binder: &BinderState, key: &str| {
            binder.module_export_equals_non_module.get(key).copied()
        };

        if let Some(target_idx) = self.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.get_binder_for_file(target_idx)
        {
            for candidate in &candidates {
                if let Some(non_module) = lookup_cached(target_binder, candidate) {
                    return non_module;
                }
            }
        }

        for candidate in &candidates {
            if let Some(non_module) = lookup_cached(self.binder, candidate) {
                return non_module;
            }
        }

        if let Some(all_binders) = self.all_binders.as_ref() {
            for binder in all_binders.iter() {
                for candidate in &candidates {
                    if let Some(non_module) = lookup_cached(binder, candidate) {
                        return non_module;
                    }
                }
            }
        }

        let export_equals_is_non_module = |binder: &BinderState,
                                           exports: &tsz_binder::SymbolTable|
         -> Option<bool> {
            let export_equals_sym_id = exports.get("export=")?;
            let has_named_exports = exports.iter().any(|(name, _)| name != "export=");
            tracing::trace!(
                module_specifier = module_specifier,
                export_equals_sym_id = export_equals_sym_id.0,
                has_named_exports,
                "module_resolves_to_non_module_entity: checking exports table"
            );

            let mut candidate_symbols = Vec::with_capacity(2);
            if let Some(sym) = binder.get_symbol(export_equals_sym_id) {
                candidate_symbols.push((binder, sym));
            } else if let Some(sym) = self.binder.get_symbol(export_equals_sym_id) {
                candidate_symbols.push((self.binder, sym));
            } else if let Some(all_binders) = self.all_binders.as_ref() {
                for other in all_binders.iter() {
                    if let Some(sym) = other.get_symbol(export_equals_sym_id) {
                        candidate_symbols.push((other.as_ref(), sym));
                        break;
                    }
                }
            }

            let has_namespace_shape = |sym_binder: &BinderState, sym: &tsz_binder::Symbol| {
                let has_namespace_decl = sym.declarations.iter().any(|decl_idx| {
                    if decl_idx.is_none() {
                        return false;
                    }
                    sym_binder
                        .declaration_arenas
                        .get(&(sym.id, *decl_idx))
                        .and_then(|v| v.first())
                        .is_some_and(|arena| {
                            let Some(node) = arena.get(*decl_idx) else {
                                return false;
                            };
                            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                                return false;
                            }
                            let Some(module_decl) = arena.get_module(node) else {
                                return false;
                            };
                            if module_decl.body.is_none() {
                                return false;
                            }
                            let Some(body_node) = arena.get(module_decl.body) else {
                                return false;
                            };
                            if body_node.kind == syntax_kind_ext::MODULE_BLOCK
                                && let Some(block) = arena.get_module_block(body_node)
                                && let Some(statements) = block.statements.as_ref()
                            {
                                return !statements.nodes.is_empty();
                            }
                            true
                        })
                });

                sym.exports.as_ref().is_some_and(|tbl| !tbl.is_empty())
                    || sym.members.as_ref().is_some_and(|tbl| !tbl.is_empty())
                    || has_namespace_decl
            };

            let export_assignment_target_name =
                |sym_binder: &BinderState, sym: &tsz_binder::Symbol| -> Option<String> {
                    let mut decls = sym.declarations.clone();
                    if !sym.value_declaration.is_none() {
                        decls.push(sym.value_declaration);
                    }

                    for decl_idx in decls {
                        if decl_idx.is_none() {
                            continue;
                        }
                        let Some(arena) = sym_binder
                            .declaration_arenas
                            .get(&(sym.id, decl_idx))
                            .and_then(|v| v.first())
                        else {
                            continue;
                        };
                        let Some(node) = arena.get(decl_idx) else {
                            continue;
                        };
                        if node.kind != syntax_kind_ext::EXPORT_ASSIGNMENT {
                            continue;
                        }
                        let Some(assign) = arena.get_export_assignment(node) else {
                            continue;
                        };
                        if !assign.is_export_equals {
                            continue;
                        }
                        let Some(expr_node) = arena.get(assign.expression) else {
                            continue;
                        };
                        if let Some(id) = arena.get_identifier(expr_node) {
                            return Some(id.escaped_text.clone());
                        }
                    }

                    None
                };

            let symbol_has_namespace_shape =
                candidate_symbols.into_iter().any(|(sym_binder, sym)| {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        symbol_name = sym.escaped_name.as_str(),
                        symbol_flags = sym.flags,
                        "module_resolves_to_non_module_entity: candidate symbol"
                    );
                    if has_namespace_shape(sym_binder, sym) {
                        return true;
                    }

                    if sym_binder
                        .get_symbols()
                        .find_all_by_name(&sym.escaped_name)
                        .into_iter()
                        .filter_map(|candidate_id| sym_binder.get_symbol(candidate_id))
                        .any(|candidate| has_namespace_shape(sym_binder, candidate))
                    {
                        return true;
                    }

                    let Some(target_name) = export_assignment_target_name(sym_binder, sym) else {
                        return false;
                    };
                    tracing::trace!(
                        module_specifier = module_specifier,
                        target_name = target_name.as_str(),
                        "module_resolves_to_non_module_entity: export assignment target"
                    );

                    sym_binder
                        .get_symbols()
                        .find_all_by_name(&target_name)
                        .into_iter()
                        .filter_map(|target_sym_id| sym_binder.get_symbol(target_sym_id))
                        .any(|target_sym| has_namespace_shape(sym_binder, target_sym))
                });

            tracing::trace!(
                module_specifier = module_specifier,
                symbol_has_namespace_shape,
                "module_resolves_to_non_module_entity: namespace shape computed"
            );
            Some(!has_named_exports && !symbol_has_namespace_shape)
        };
        let has_namespace_shape = |binder: &BinderState, sym: &tsz_binder::Symbol| {
            let has_namespace_decl = sym.declarations.iter().any(|decl_idx| {
                if decl_idx.is_none() {
                    return false;
                }
                binder
                    .declaration_arenas
                    .get(&(sym.id, *decl_idx))
                    .and_then(|v| v.first())
                    .is_some_and(|arena| {
                        let Some(node) = arena.get(*decl_idx) else {
                            return false;
                        };
                        if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                            return false;
                        }
                        let Some(module_decl) = arena.get_module(node) else {
                            return false;
                        };
                        if module_decl.body.is_none() {
                            return false;
                        }
                        let Some(body_node) = arena.get(module_decl.body) else {
                            return false;
                        };
                        if body_node.kind == syntax_kind_ext::MODULE_BLOCK
                            && let Some(block) = arena.get_module_block(body_node)
                            && let Some(statements) = block.statements.as_ref()
                        {
                            return !statements.nodes.is_empty();
                        }
                        true
                    })
            });

            sym.exports.as_ref().is_some_and(|tbl| !tbl.is_empty())
                || sym.members.as_ref().is_some_and(|tbl| !tbl.is_empty())
                || has_namespace_decl
        };
        fn contains_namespace_decl_named(
            arena: &NodeArena,
            idx: NodeIndex,
            target_name: &str,
            depth: usize,
        ) -> bool {
            if depth > 128 {
                return false;
            }
            let Some(node) = arena.get(idx) else {
                return false;
            };

            if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                let Some(module_decl) = arena.get_module(node) else {
                    return false;
                };
                if let Some(name_node) = arena.get(module_decl.name)
                    && let Some(id) = arena.get_identifier(name_node)
                    && id.escaped_text == target_name
                {
                    if module_decl.body.is_none() {
                        return false;
                    }
                    if let Some(body_node) = arena.get(module_decl.body)
                        && body_node.kind == syntax_kind_ext::MODULE_BLOCK
                        && let Some(block) = arena.get_module_block(body_node)
                        && let Some(stmts) = block.statements.as_ref()
                    {
                        return !stmts.nodes.is_empty();
                    }
                    return true;
                }
                if !module_decl.body.is_none() {
                    return contains_namespace_decl_named(
                        arena,
                        module_decl.body,
                        target_name,
                        depth + 1,
                    );
                }
                return false;
            }

            if node.kind == syntax_kind_ext::MODULE_BLOCK
                && let Some(block) = arena.get_module_block(node)
                && let Some(statements) = block.statements.as_ref()
            {
                for &stmt in &statements.nodes {
                    if contains_namespace_decl_named(arena, stmt, target_name, depth + 1) {
                        return true;
                    }
                }
            }

            false
        }
        fn collect_export_equals_targets(
            arena: &NodeArena,
            idx: NodeIndex,
            out: &mut Vec<String>,
            depth: usize,
        ) {
            if depth > 128 {
                return;
            }
            let Some(node) = arena.get(idx) else {
                return;
            };

            if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                if let Some(assign) = arena.get_export_assignment(node)
                    && assign.is_export_equals
                    && let Some(expr_node) = arena.get(assign.expression)
                    && let Some(id) = arena.get_identifier(expr_node)
                {
                    out.push(id.escaped_text.clone());
                }
                return;
            }

            if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                if let Some(module_decl) = arena.get_module(node)
                    && !module_decl.body.is_none()
                {
                    collect_export_equals_targets(arena, module_decl.body, out, depth + 1);
                }
                return;
            }

            if node.kind == syntax_kind_ext::MODULE_BLOCK
                && let Some(block) = arena.get_module_block(node)
                && let Some(statements) = block.statements.as_ref()
            {
                for &stmt in &statements.nodes {
                    collect_export_equals_targets(arena, stmt, out, depth + 1);
                }
            }
        }
        let export_assignment_targets_namespace_via_source =
            |binder: &BinderState, arena: &NodeArena| {
                for source_file in &arena.source_files {
                    let mut export_targets = Vec::new();
                    for &stmt_idx in &source_file.statements.nodes {
                        collect_export_equals_targets(arena, stmt_idx, &mut export_targets, 0);
                    }
                    for target_name in export_targets {
                        let has_matching_namespace_decl = source_file
                            .statements
                            .nodes
                            .iter()
                            .copied()
                            .any(|top_stmt| {
                                contains_namespace_decl_named(arena, top_stmt, &target_name, 0)
                            });
                        if has_matching_namespace_decl {
                            return true;
                        }
                        if binder
                            .get_symbols()
                            .find_all_by_name(&target_name)
                            .into_iter()
                            .filter_map(|target_id| binder.get_symbol(target_id))
                            .any(|target_sym| has_namespace_shape(binder, target_sym))
                        {
                            return true;
                        }
                    }
                }
                false
            };

        if let Some(target_idx) = self.resolve_import_target(module_specifier)
            && let Some(target_binder) = self.get_binder_for_file(target_idx)
        {
            let target_arena = self.get_arena_for_file(target_idx as u32);
            for candidate in &candidates {
                if let Some(exports) = target_binder.module_exports.get(candidate)
                    && let Some(non_module) = export_equals_is_non_module(target_binder, exports)
                {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        candidate = candidate.as_str(),
                        branch = "target_specifier_key",
                        non_module,
                        "module_resolves_to_non_module_entity: branch result"
                    );
                    if non_module
                        && export_assignment_targets_namespace_via_source(
                            target_binder,
                            target_arena,
                        )
                    {
                        tracing::trace!(
                            module_specifier = module_specifier,
                            candidate = candidate.as_str(),
                            branch = "target_specifier_key",
                            "module_resolves_to_non_module_entity: source fallback override"
                        );
                        return false;
                    }
                    return non_module;
                }
            }

            if let Some(target_file_name) = self
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                && let Some(exports) = target_binder.module_exports.get(target_file_name)
                && let Some(non_module) = export_equals_is_non_module(target_binder, exports)
            {
                tracing::trace!(
                    module_specifier = module_specifier,
                    branch = "target_file_key",
                    non_module,
                    "module_resolves_to_non_module_entity: branch result"
                );
                if non_module
                    && export_assignment_targets_namespace_via_source(target_binder, target_arena)
                {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        branch = "target_file_key",
                        "module_resolves_to_non_module_entity: source fallback override"
                    );
                    return false;
                }
                return non_module;
            }
        }

        let mut saw_non_module = false;
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(non_module) = export_equals_is_non_module(self.binder, exports)
        {
            tracing::trace!(
                module_specifier = module_specifier,
                branch = "self_binder",
                non_module,
                "module_resolves_to_non_module_entity: branch result"
            );
            if non_module && export_assignment_targets_namespace_via_source(self.binder, self.arena)
            {
                tracing::trace!(
                    module_specifier = module_specifier,
                    branch = "self_binder",
                    "module_resolves_to_non_module_entity: source fallback override"
                );
                return false;
            }
            if !non_module {
                return false;
            }
            saw_non_module = true;
        }

        if let Some(all_binders) = self.all_binders.as_ref() {
            for (idx, binder) in all_binders.iter().enumerate() {
                if let Some(exports) = binder.module_exports.get(module_specifier)
                    && let Some(non_module) = export_equals_is_non_module(binder, exports)
                {
                    tracing::trace!(
                        module_specifier = module_specifier,
                        branch = "all_binders",
                        binder_idx = idx,
                        non_module,
                        "module_resolves_to_non_module_entity: branch result"
                    );
                    if non_module
                        && let Some(all_arenas) = self.all_arenas.as_ref()
                        && let Some(arena) = all_arenas.get(idx)
                        && export_assignment_targets_namespace_via_source(binder, arena.as_ref())
                    {
                        tracing::trace!(
                            module_specifier = module_specifier,
                            branch = "all_binders",
                            binder_idx = idx,
                            "module_resolves_to_non_module_entity: source fallback override"
                        );
                        return false;
                    }
                    if !non_module {
                        return false;
                    }
                    saw_non_module = true;
                }
            }
        }

        saw_non_module
    }

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        TypeCache {
            symbol_types: self.symbol_types,
            symbol_instance_types: self.symbol_instance_types,
            node_types: self.node_types,
            symbol_dependencies: self.symbol_dependencies,
            def_to_symbol: self.def_to_symbol.into_inner(),
            flow_analysis_cache: self.flow_analysis_cache.into_inner(),
            class_instance_type_to_decl: self.class_instance_type_to_decl,
            class_instance_type_cache: self.class_instance_type_cache,
            type_only_nodes: self.type_only_nodes,
        }
    }

    /// Add an error diagnostic (with deduplication).
    /// Diagnostics with the same (start, code) are only emitted once.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        // Check if we've already emitted this diagnostic
        let key = (start, code);
        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code,
            start,
            length,
            file = %self.file_name,
            message = %message,
            "diagnostic"
        );
        self.diagnostics.push(Diagnostic::error(
            self.file_name.clone(),
            start,
            length,
            message,
            code,
        ));
    }

    /// Push a diagnostic with deduplication.
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exception: TS2318 (missing global type) at position 0 uses message hash
    /// to allow multiple distinct global type errors.
    pub fn push_diagnostic(&mut self, diag: Diagnostic) {
        // For TS2318 at position 0, include message hash in key to allow distinct errors
        // (e.g., "Cannot find global type 'Array'" vs "Cannot find global type 'Object'")
        let key = if diag.code == 2318 && diag.start == 0 {
            // Use a hash of the message to distinguish different TS2318 errors
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            diag.message_text.hash(&mut hasher);
            (hasher.finish() as u32, diag.code)
        } else {
            (diag.start, diag.code)
        };

        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code = diag.code,
            start = diag.start,
            length = diag.length,
            file = %diag.file,
            message = %diag.message_text,
            "diagnostic"
        );
        self.diagnostics.push(diag);
    }

    /// Get node span (pos, end) from index.
    pub fn get_node_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.arena.get(idx)?;
        Some((node.pos, node.end))
    }

    /// Push an expected return type onto the stack.
    pub fn push_return_type(&mut self, return_type: TypeId) {
        self.return_type_stack.push(return_type);
    }

    /// Pop the expected return type from the stack.
    pub fn pop_return_type(&mut self) {
        self.return_type_stack.pop();
    }

    /// Get the current expected return type.
    pub fn current_return_type(&self) -> Option<TypeId> {
        self.return_type_stack.last().copied()
    }

    /// Push a contextual yield type for a generator function.
    pub fn push_yield_type(&mut self, yield_type: Option<TypeId>) {
        self.yield_type_stack.push(yield_type);
    }

    /// Pop the contextual yield type from the stack.
    pub fn pop_yield_type(&mut self) {
        self.yield_type_stack.pop();
    }

    /// Get the current contextual yield type for the enclosing generator.
    pub fn current_yield_type(&self) -> Option<TypeId> {
        self.yield_type_stack.last().copied().flatten()
    }

    /// Enter an async context (increment async depth).
    pub const fn enter_async_context(&mut self) {
        self.async_depth += 1;
    }

    /// Exit an async context (decrement async depth).
    pub const fn exit_async_context(&mut self) {
        if self.async_depth > 0 {
            self.async_depth -= 1;
        }
    }

    /// Check if we're currently inside an async function.
    pub const fn in_async_context(&self) -> bool {
        self.async_depth > 0
    }

    /// Consume one unit of type resolution fuel.
    /// Returns true if fuel is still available, false if exhausted.
    /// When exhausted, type resolution should return ERROR to prevent timeout.
    pub fn consume_fuel(&self) -> bool {
        let mut fuel = self.type_resolution_fuel.borrow_mut();
        if *fuel == 0 {
            *self.fuel_exhausted.borrow_mut() = true;
            return false;
        }
        *fuel -= 1;
        true
    }

    /// Check if type resolution fuel has been exhausted.
    pub fn is_fuel_exhausted(&self) -> bool {
        *self.fuel_exhausted.borrow()
    }

    /// Enter a recursive call. Returns true if recursion is allowed,
    /// false if the depth limit has been reached (caller should bail out).
    #[inline]
    pub fn enter_recursion(&self) -> bool {
        self.recursion_depth.borrow_mut().enter()
    }

    /// Leave a recursive call (decrement depth counter).
    #[inline]
    pub fn leave_recursion(&self) {
        self.recursion_depth.borrow_mut().leave();
    }

    /// Check if Promise is available in lib files or global scope.
    /// Returns true if Promise is declared in lib contexts, globals, or type declarations.
    pub fn has_promise_in_lib(&self) -> bool {
        // Check lib contexts first
        for lib_ctx in &self.lib_contexts {
            if lib_ctx.binder.file_locals.has("Promise") {
                return true;
            }
        }

        // Check if Promise is available in current scope/global context
        if self.binder.current_scope.has("Promise") {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has("Promise") {
            return true;
        }

        false
    }

    /// Check if the Promise constructor VALUE is available.
    /// This is different from `has_promise_in_lib()` which checks for the type.
    /// The ES5 lib declares `interface Promise<T>` (type only) but NOT
    /// `declare var Promise: PromiseConstructor` (value). ES2015+ libs declare both.
    /// Used for TS2705: "An async function in ES5 requires the Promise constructor."
    pub fn has_promise_constructor_in_scope(&self) -> bool {
        use tsz_binder::symbol_flags;
        // Fast-path: if PromiseConstructor type is present in loaded libs/scope,
        // treat Promise constructor as available even if VALUE flags were not merged.
        if self.has_name_in_lib("PromiseConstructor") {
            return true;
        }
        // Check if Promise exists as a VALUE symbol (not just a TYPE)
        let check_symbol_has_value =
            |sym_id: tsz_binder::SymbolId, binder: &tsz_binder::BinderState| -> bool {
                if let Some(sym) = binder.symbols.get(sym_id) {
                    (sym.flags & symbol_flags::VALUE) != 0
                } else {
                    false
                }
            };

        // Check lib contexts
        for lib_ctx in &self.lib_contexts {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get("Promise")
                && check_symbol_has_value(sym_id, &lib_ctx.binder)
            {
                return true;
            }
        }

        // Check current scope
        if let Some(sym_id) = self.binder.current_scope.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        // Check file locals
        if let Some(sym_id) = self.binder.file_locals.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        false
    }

    /// Check if Symbol is available in lib files or global scope.
    /// Returns true if Symbol is declared in lib contexts, globals, or type declarations.
    pub fn has_symbol_in_lib(&self) -> bool {
        // Check lib contexts first
        for lib_ctx in &self.lib_contexts {
            if lib_ctx.binder.file_locals.has("Symbol") {
                return true;
            }
        }

        // Check if Symbol is available in current scope/global context
        if self.binder.current_scope.has("Symbol") {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has("Symbol") {
            return true;
        }

        false
    }

    /// Check if a named symbol is available in lib files or global scope.
    /// Returns true if the symbol is declared in lib contexts, globals, or current scope.
    /// This is a generalized version of `has_symbol_in_lib` for any symbol name.
    pub fn has_name_in_lib(&self, name: &str) -> bool {
        // Check lib contexts first
        for lib_ctx in &self.lib_contexts {
            if lib_ctx.binder.file_locals.has(name) {
                return true;
            }
        }

        // Check if symbol is available in current scope/global context
        if self.binder.current_scope.has(name) {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has(name) {
            return true;
        }

        false
    }

    /// Check if a symbol originates from a lib context.
    pub fn symbol_is_from_lib(&self, sym_id: SymbolId) -> bool {
        let Some(symbol_arena) = self.binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        self.lib_contexts
            .iter()
            .any(|lib_ctx| Arc::ptr_eq(&lib_ctx.arena, symbol_arena))
    }

    /// Check if a name is a known global type that should emit TS2318/TS2583 when missing.
    /// This helps distinguish between "unknown name" (TS2304) and "missing global type" (TS2318/TS2583).
    pub fn is_known_global_type(&self, name: &str) -> bool {
        use tsz_binder::lib_loader;

        // ES2015+ types
        if lib_loader::is_es2015_plus_type(name) {
            return true;
        }

        // Pre-ES2015 global types that are commonly used
        // These are always available in lib.d.ts but should emit TS2318 when @noLib is enabled
        matches!(
            name,
            "Object"
                | "Function"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Date"
                | "RegExp"
                | "Error"
                | "Math"
                | "JSON"
                | "console"
                | "window"
                | "document"
                | "ArrayBuffer"
                | "DataView"
                | "Int8Array"
                | "Uint8Array"
                | "Uint8ClampedArray"
                | "Int16Array"
                | "Uint16Array"
                | "Int32Array"
                | "Uint32Array"
                | "Float32Array"
                | "Float64Array"
                | "this"
                | "globalThis"
                | "IArguments"
        )
    }

    /// Check if a global type is missing due to insufficient ES version support.
    /// Returns the minimum ES version required for this type, or None if not applicable.
    pub fn get_required_es_version_for_global(&self, name: &str) -> Option<&'static str> {
        use tsz_binder::lib_loader;

        if lib_loader::is_es2015_plus_type(name) {
            return Some("ES2015");
        }

        // Most pre-ES2015 globals are available in ES3/ES5
        match name {
            "Promise" | "Map" | "Set" | "WeakMap" | "WeakSet" | "Proxy" | "Reflect" | "Symbol"
            | "Iterator" | "Iterable" => Some("ES2015"),
            "AsyncFunction" | "SharedArrayBuffer" | "Atomics" => Some("ES2017"),
            "AsyncGenerator" | "AsyncGeneratorFunction" => Some("ES2018"),
            "BigInt" | "BigInt64Array" | "BigUint64Array" => Some("ES2020"),
            "FinalizationRegistry" | "WeakRef" => Some("ES2021"),
            _ => None,
        }
    }

    /// Check if a modifier list contains a specific modifier kind.
    pub fn has_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
        kind: u16,
    ) -> bool {
        if let Some(mods) = modifiers {
            for &idx in &mods.nodes {
                if let Some(node) = self.arena.get(idx)
                    && node.kind == kind
                {
                    return true;
                }
            }
        }
        false
    }

    // =========================================================================
    // Flow Graph Queries
    // =========================================================================

    /// Check flow usage at a specific AST node.
    ///
    /// This method queries the control flow graph to determine flow-sensitive
    /// information at a given node. Returns `None` if flow graph is not available.
    ///
    /// # Arguments
    /// * `node_idx` - The AST node to query flow information for
    ///
    /// # Returns
    /// * `Some(FlowNodeId)` - The flow node ID at this location
    /// * `None` - If flow graph is not available or node has no flow info
    pub fn check_flow_usage(&self, node_idx: NodeIndex) -> Option<tsz_binder::FlowNodeId> {
        if let Some(ref _graph) = self.flow_graph {
            // Look up the flow node for this AST node from the binder's node_flow mapping
            self.binder.node_flow.get(&node_idx.0).copied()
        } else {
            None
        }
    }

    /// Get a reference to the flow graph.
    pub const fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }

    // =========================================================================
    // Compiler Option Accessors
    // =========================================================================

    /// Check if strict mode is enabled.
    pub const fn is_strict_mode(&self) -> bool {
        self.compiler_options.strict
    }

    /// Check if noImplicitAny is enabled for the current file.
    /// For JavaScript files, noImplicitAny only applies when checkJs is also enabled.
    /// This allows TS7006 to fire in .js files with --checkJs --strict.
    pub fn no_implicit_any(&self) -> bool {
        if !self.compiler_options.no_implicit_any {
            return false;
        }

        let is_js_file = self.file_name.ends_with(".js")
            || self.file_name.ends_with(".jsx")
            || self.file_name.ends_with(".mjs")
            || self.file_name.ends_with(".cjs");

        // JS files get noImplicitAny errors only when checkJs is enabled
        if is_js_file {
            self.compiler_options.check_js
        } else {
            true
        }
    }

    /// Check if noImplicitReturns is enabled.
    pub const fn no_implicit_returns(&self) -> bool {
        self.compiler_options.no_implicit_returns
    }

    /// Check if noImplicitThis is enabled.
    pub const fn no_implicit_this(&self) -> bool {
        self.compiler_options.no_implicit_this
    }

    /// Check if noImplicitOverride is enabled.
    pub const fn no_implicit_override(&self) -> bool {
        self.compiler_options.no_implicit_override
    }

    /// Check if strictNullChecks is enabled.
    pub const fn strict_null_checks(&self) -> bool {
        self.compiler_options.strict_null_checks
    }

    /// Check if strictFunctionTypes is enabled.
    pub const fn strict_function_types(&self) -> bool {
        self.compiler_options.strict_function_types
    }

    /// Check if strictPropertyInitialization is enabled.
    pub const fn strict_property_initialization(&self) -> bool {
        self.compiler_options.strict_property_initialization
    }

    /// Check if useUnknownInCatchVariables is enabled.
    pub const fn use_unknown_in_catch_variables(&self) -> bool {
        self.compiler_options.use_unknown_in_catch_variables
    }

    /// Check if isolatedModules is enabled.
    pub const fn isolated_modules(&self) -> bool {
        self.compiler_options.isolated_modules
    }

    /// Check if noUncheckedIndexedAccess is enabled.
    /// When enabled, index signature access adds `| undefined` to the result type.
    pub const fn no_unchecked_indexed_access(&self) -> bool {
        self.compiler_options.no_unchecked_indexed_access
    }

    /// Check if strictBindCallApply is enabled.
    /// When enabled, bind/call/apply use strict function signatures.
    pub const fn strict_bind_call_apply(&self) -> bool {
        self.compiler_options.strict_bind_call_apply
    }

    /// Check if exactOptionalPropertyTypes is enabled.
    /// When enabled, optional properties are `T | undefined` not `T | undefined | missing`.
    pub const fn exact_optional_property_types(&self) -> bool {
        self.compiler_options.exact_optional_property_types
    }

    /// Check if sound mode is enabled.
    pub const fn sound_mode(&self) -> bool {
        self.compiler_options.sound_mode
    }

    /// Pack the checker's compiler options into a `u16` bitmask for use as a
    /// `RelationCacheKey` flags field. This is the single source of truth for
    /// flag packing — call this instead of manually constructing the bitmask.
    pub const fn pack_relation_flags(&self) -> u16 {
        use tsz_solver::RelationCacheKey;
        let mut flags: u16 = 0;
        if self.strict_null_checks() {
            flags |= RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
        }
        if self.strict_function_types() {
            flags |= RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
        }
        if self.exact_optional_property_types() {
            flags |= RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES;
        }
        if self.no_unchecked_indexed_access() {
            flags |= RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS;
        }
        flags
    }

    /// Convert `CheckerOptions` to `JudgeConfig` for the `CompatChecker`.
    const fn as_judge_config(&self) -> JudgeConfig {
        JudgeConfig {
            strict_function_types: self.strict_function_types(),
            strict_null_checks: self.strict_null_checks(),
            exact_optional_property_types: self.exact_optional_property_types(),
            no_unchecked_indexed_access: self.no_unchecked_indexed_access(),
            sound_mode: self.sound_mode(),
        }
    }

    /// Apply standard compiler options to a `CompatChecker`, including `query_db`.
    /// This wires the `CompilerOptions` (via `JudgeConfig`) and the `QueryDatabase`.
    pub fn configure_compat_checker<'b, R: tsz_solver::TypeResolver>(
        &'b self,
        checker: &mut tsz_solver::CompatChecker<'b, R>,
    ) {
        // Apply configuration from options
        checker.apply_config(&self.as_judge_config());

        // Set the query database for memoization/interning
        checker.set_query_db(self.types);

        // Set the inheritance graph for nominal class subtype checking
        checker.set_inheritance_graph(Some(&self.inheritance_graph));

        // Configure strict subtype checking if Sound Mode is enabled
        if self.compiler_options.sound_mode {
            checker.set_strict_subtype_checking(true);
            checker.set_strict_any_propagation(true);
        }
    }

    /// Check if noUnusedLocals is enabled.
    pub const fn no_unused_locals(&self) -> bool {
        self.compiler_options.no_unused_locals
    }

    /// Check if noUnusedParameters is enabled.
    pub const fn no_unused_parameters(&self) -> bool {
        self.compiler_options.no_unused_parameters
    }

    /// Check if noLib is enabled.
    /// When enabled, no library files (including lib.d.ts) are included.
    /// TS2318 errors are emitted when referencing global types with this option enabled.
    pub const fn no_lib(&self) -> bool {
        self.compiler_options.no_lib
    }

    /// Check if lib files are loaded (lib.d.ts, etc.).
    /// Returns false when noLib is enabled or when no actual lib files are loaded.
    /// Uses `actual_lib_file_count` instead of `lib_contexts.is_empty()` because `lib_contexts`
    /// may also contain user file contexts for cross-file resolution in multi-file tests.
    /// Used to determine whether to emit TS2304/TS2318/TS2583 for missing global types.
    pub const fn has_lib_loaded(&self) -> bool {
        !self.compiler_options.no_lib && self.actual_lib_file_count > 0
    }

    /// Check if esModuleInterop is enabled.
    /// When enabled, synthesizes default exports for `CommonJS` modules.
    pub const fn es_module_interop(&self) -> bool {
        self.compiler_options.es_module_interop
    }

    /// Check if allowSyntheticDefaultImports is enabled.
    /// When enabled, allows `import x from 'y'` when module doesn't have default export.
    /// This is implied by esModuleInterop.
    pub const fn allow_synthetic_default_imports(&self) -> bool {
        self.compiler_options.allow_synthetic_default_imports
    }
}
