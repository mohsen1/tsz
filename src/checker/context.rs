//! Checker Context
//!
//! Holds the shared state used throughout the type checking process.
//! This separates state from logic, allowing specialized checkers (expressions, statements)
//! to borrow the context mutably.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use crate::binder::SymbolId;
use crate::checker::control_flow::FlowGraph;
use crate::checker::types::diagnostics::Diagnostic;
use crate::common::ModuleKind;
use crate::parser::NodeIndex;
use crate::solver::def::{DefId, DefinitionStore};
use crate::solver::types::RelationCacheKey;
use crate::solver::{PropertyInfo, QueryDatabase, TypeEnvironment, TypeId, judge::JudgeConfig};

/// Compiler options for type checking.
#[derive(Debug, Clone, Default)]
pub struct CheckerOptions {
    pub strict: bool,
    pub no_implicit_any: bool,
    pub no_implicit_returns: bool,
    pub strict_null_checks: bool,
    pub strict_function_types: bool,
    pub strict_property_initialization: bool,
    pub no_implicit_this: bool,
    pub use_unknown_in_catch_variables: bool,
    pub isolated_modules: bool,
    /// When true, indexed access with index signatures adds `| undefined` to the type
    pub no_unchecked_indexed_access: bool,
    /// When true, checking bind/call/apply uses strict function signatures
    pub strict_bind_call_apply: bool,
    /// When true, optional properties are treated as exactly `T | undefined` not `T | undefined | missing`
    pub exact_optional_property_types: bool,
    /// When true, no library files (including lib.d.ts) are included.
    /// This corresponds to the --noLib compiler flag.
    /// TS2318 errors are emitted when referencing global types with this option enabled.
    pub no_lib: bool,
    /// When true, do not automatically inject built-in type declarations.
    /// This corresponds to the --noTypesAndSymbols compiler flag.
    /// Prevents loading default lib.d.ts files which provide types like Array, Object, etc.
    pub no_types_and_symbols: bool,
    /// Target ECMAScript version (ES3, ES5, ES2015, ES2016, etc.)
    /// Controls which built-in types are available (e.g., Promise requires ES2015)
    /// Defaults to ES3 for maximum compatibility
    pub target: ScriptTarget,
    /// Module kind (None, CommonJS, ES2015, ES2020, ES2022, ESNext, etc.)
    /// Controls which module system is being targeted (affects import/export syntax validity)
    pub module: ModuleKind,
    /// Emit additional JavaScript to ease support for importing CommonJS modules.
    /// When true, synthesizes default exports for CommonJS modules.
    pub es_module_interop: bool,
    /// Allow 'import x from y' when a module doesn't have a default export.
    /// Implied by esModuleInterop.
    pub allow_synthetic_default_imports: bool,
    /// When true, disable error reporting for unreachable code (TS7027).
    pub allow_unreachable_code: bool,
    /// When true, require bracket notation for index signature property access (TS4111).
    pub no_property_access_from_index_signature: bool,
    /// When true, enable Sound Mode for stricter type checking beyond TypeScript's defaults.
    /// Sound Mode catches common unsoundness issues like:
    /// - Mutable array covariance (TS9002)
    /// - Method parameter bivariance (TS9003)
    /// - `any` escapes (TS9004)
    /// - Excess properties via sticky freshness (TS9001)
    ///
    /// Activated via: `--sound` CLI flag or `// @ts-sound` pragma
    pub sound_mode: bool,
    /// When true, enables experimental support for decorators (legacy decorators).
    /// This is required for the @experimentalDecorators flag.
    /// When decorators are used, TypedPropertyDescriptor must be available.
    pub experimental_decorators: bool,
    /// When true, report errors for unused local variables (TS6133).
    pub no_unused_locals: bool,
    /// When true, report errors for unused function parameters (TS6133).
    pub no_unused_parameters: bool,
}

/// ECMAScript target version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptTarget {
    /// ECMAScript 3 (earliest version, minimal built-ins)
    ES3,
    /// ECMAScript 5 (adds strict mode, JSON, Date, RegExp, etc.)
    ES5,
    /// ECMAScript 2015 (ES6) - adds Promise, Map, Set, Symbol, etc.
    ES2015,
    /// ECMAScript 2016 - adds Array.prototype.includes, ** operator
    ES2016,
    /// ECMAScript 2017 - adds async/await, Object.values, etc.
    ES2017,
    /// ECMAScript 2018 - adds spread properties, rest properties, etc.
    ES2018,
    /// ECMAScript 2019 - adds Array.prototype.flat, etc.
    ES2019,
    /// ECMAScript 2020 - adds optional chaining, nullish coalescing, etc.
    ES2020,
    /// Latest supported ECMAScript features
    #[default]
    ESNext,
}

impl ScriptTarget {
    /// Check if this target supports ES2015+ features (Promise, Map, Set, Symbol, etc.)
    pub fn supports_es2015(&self) -> bool {
        matches!(
            self,
            Self::ES2015
                | Self::ES2016
                | Self::ES2017
                | Self::ES2018
                | Self::ES2019
                | Self::ES2020
                | Self::ESNext
        )
    }
}

impl CheckerOptions {
    /// Apply TypeScript's `--strict` defaults to individual strict flags.
    /// In tsc, enabling `strict` turns on the strict family unless explicitly disabled.
    /// We mirror that behavior by OR-ing the per-flag booleans with `strict`.
    pub fn apply_strict_defaults(mut self) -> Self {
        if self.strict {
            self.no_implicit_any = true;
            self.no_implicit_this = true;
            self.strict_null_checks = true;
            self.strict_function_types = true;
            self.strict_bind_call_apply = true;
            self.strict_property_initialization = true;
            self.use_unknown_in_catch_variables = true;
            // exactOptionalPropertyTypes and other opts are not implied by --strict
        }
        self
    }
}
use crate::binder::BinderState;
use crate::parser::node::NodeArena;

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
}

/// Persistent cache for type checking results across LSP queries.
/// This cache survives between LSP requests but is invalidated when the file changes.
#[derive(Clone, Debug)]
pub struct TypeCache {
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached instance types for class symbols (for TYPE position).
    /// Distinguishes from symbol_types which holds constructor types for VALUE position.
    pub symbol_instance_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Cache for type relation results (subtype checking).
    /// Uses RelationCacheKey to ensure Lawyer-layer configuration (strict vs non-strict)
    /// doesn't cause cache poisoning.
    pub relation_cache: FxHashMap<RelationCacheKey, bool>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Cached abstract constructor types (TypeIds) for assignability checks.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Cached protected constructor types (TypeIds) for assignability checks.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Cached private constructor types (TypeIds) for assignability checks.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// Maps DefIds to SymbolIds for declaration emit usage analysis.
    /// Populated by CheckerContext during type checking, consumed by UsageAnalyzer.
    pub def_to_symbol: FxHashMap<crate::solver::DefId, SymbolId>,

    // === Specialized Caches (moved from CheckerContext) ===
    /// Cache for evaluated application types to avoid repeated expansion.
    pub application_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Cache for evaluated mapped types with symbol resolution.
    pub mapped_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

    /// Cache for object spread property collection.
    pub object_spread_property_cache: FxHashMap<TypeId, Vec<PropertyInfo>>,

    /// Recursion guard for object spread property collection.
    pub object_spread_property_set: FxHashSet<TypeId>,

    /// Cache for element access type computation.
    pub element_access_type_cache: FxHashMap<(TypeId, TypeId, Option<usize>), TypeId>,

    /// Recursion guard for element access type computation.
    pub element_access_type_set: FxHashSet<(TypeId, TypeId, Option<usize>)>,

    /// Cache for control flow analysis results.
    /// Key: (FlowNodeId, SymbolId, InitialTypeId) -> NarrowedTypeId
    pub flow_analysis_cache:
        FxHashMap<(crate::binder::FlowNodeId, crate::binder::SymbolId, TypeId), TypeId>,

    /// Maps class instance TypeIds to their class declaration NodeIndex.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration NodeIndex -> computed instance TypeId.
    /// Avoids recomputing the full class instance type on every member check.
    pub class_instance_type_cache: FxHashMap<NodeIndex, TypeId>,
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
        self.abstract_constructor_types.clear();
        self.protected_constructor_types.clear();
        self.private_constructor_types.clear();

        affected.len()
    }

    /// Merge another TypeCache into this one.
    /// Used to accumulate type information from multiple file checks for declaration emit.
    pub fn merge(&mut self, other: TypeCache) {
        self.symbol_types.extend(other.symbol_types);
        self.symbol_instance_types
            .extend(other.symbol_instance_types);
        self.node_types.extend(other.node_types);
        self.relation_cache.extend(other.relation_cache);

        // Merge symbol dependencies sets
        for (sym, deps) in other.symbol_dependencies {
            self.symbol_dependencies
                .entry(sym)
                .or_default()
                .extend(deps);
        }

        self.abstract_constructor_types
            .extend(other.abstract_constructor_types);
        self.protected_constructor_types
            .extend(other.protected_constructor_types);
        self.private_constructor_types
            .extend(other.private_constructor_types);

        // Merge def_to_symbol mapping
        self.def_to_symbol.extend(other.def_to_symbol);
    }
}

/// Shared state for type checking.
pub struct CheckerContext<'a> {
    /// The NodeArena containing the AST.
    pub arena: &'a NodeArena,

    /// The binder state with symbols.
    pub binder: &'a BinderState,

    /// Query database for type interning and memoized type operations.
    /// Supports both TypeInterner (via trait upcasting) and QueryCache.
    pub types: &'a dyn QueryDatabase,

    /// Current file name.
    pub file_name: String,

    /// Compiler options for type checking.
    pub compiler_options: CheckerOptions,

    /// Whether unresolved import diagnostics should be emitted by the checker.
    /// The CLI driver handles module resolution in multi-file mode.
    pub report_unresolved_imports: bool,

    // --- Caches ---
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached instance types for class symbols (for TYPE position).
    /// Distinguishes from symbol_types which holds constructor types for VALUE position.
    pub symbol_instance_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Cache for type relation results.
    /// Uses RelationCacheKey to ensure Lawyer-layer configuration (strict vs non-strict)
    /// doesn't cause cache poisoning.
    pub relation_cache: RefCell<FxHashMap<RelationCacheKey, bool>>,

    /// Cached type environment for resolving Ref types during assignability checks.
    pub type_environment: Rc<RefCell<TypeEnvironment>>,

    /// Cache for evaluated application types to avoid repeated expansion.
    pub application_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Cache for evaluated mapped types with symbol resolution.
    pub mapped_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

    /// Cache for object spread property collection.
    pub object_spread_property_cache: FxHashMap<TypeId, Vec<PropertyInfo>>,

    /// Recursion guard for object spread property collection.
    pub object_spread_property_set: FxHashSet<TypeId>,

    /// Cache for element access type computation.
    pub element_access_type_cache: FxHashMap<(TypeId, TypeId, Option<usize>), TypeId>,

    /// Recursion guard for element access type computation.
    pub element_access_type_set: FxHashSet<(TypeId, TypeId, Option<usize>)>,

    /// Cache for control flow analysis results.
    /// Key: (FlowNodeId, SymbolId, InitialTypeId) -> NarrowedTypeId
    /// Prevents re-traversing the flow graph for the same symbol/flow combination.
    /// Fixes performance regression on binaryArithmeticControlFlowGraphNotTooLarge.ts
    /// where each operand in a + b + c was triggering fresh graph traversals.
    pub flow_analysis_cache:
        RefCell<FxHashMap<(crate::binder::FlowNodeId, crate::binder::SymbolId, TypeId), TypeId>>,

    /// Maps class instance TypeIds to their class declaration NodeIndex.
    /// Used by `get_class_decl_from_type` to correctly identify the class
    /// for derived classes that have no private/protected members (and thus no brand).
    /// Populated by `get_class_instance_type_inner` when creating class instance types.
    pub class_instance_type_to_decl: FxHashMap<TypeId, NodeIndex>,

    /// Forward cache: class declaration NodeIndex -> computed instance TypeId.
    /// Avoids recomputing the full class instance type on every member check.
    pub class_instance_type_cache: FxHashMap<NodeIndex, TypeId>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Stack of symbols currently being evaluated for dependency tracking.
    pub symbol_dependency_stack: Vec<SymbolId>,

    /// Set of symbols that have been referenced (used for TS6133 unused checking).
    /// Uses RefCell to allow tracking from &self methods (e.g., resolve_identifier_symbol).
    pub referenced_symbols: std::cell::RefCell<FxHashSet<SymbolId>>,

    // --- Diagnostics ---
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
    pub symbol_resolution_set: HashSet<SymbolId>,
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: HashSet<SymbolId>,
    /// O(1) lookup set for class constructor type resolution to avoid recursion.
    pub class_constructor_resolution_set: HashSet<SymbolId>,

    /// Inheritance graph tracking class/interface relationships
    pub inheritance_graph: crate::solver::inheritance::InheritanceGraph,

    /// Stack of nodes being resolved.
    pub node_resolution_stack: Vec<NodeIndex>,
    /// O(1) lookup set for node resolution stack.
    pub node_resolution_set: HashSet<NodeIndex>,

    // --- Scopes & Context ---
    /// Current type parameter scope.
    pub type_parameter_scope: HashMap<String, TypeId>,

    /// Contextual type for expression being checked.
    pub contextual_type: Option<TypeId>,

    /// Current depth of recursive type instantiation.
    pub instantiation_depth: RefCell<u32>,

    /// Whether type instantiation depth was exceeded (for TS2589 emission).
    pub depth_exceeded: RefCell<bool>,

    /// General recursion depth counter for type checking.
    /// Prevents stack overflow by bailing out when depth exceeds MAX_RECURSION_DEPTH.
    pub recursion_depth: Cell<u32>,

    /// Current depth of call expression resolution.
    pub call_depth: RefCell<u32>,

    /// Stack of expected return types for functions.
    pub return_type_stack: Vec<TypeId>,
    /// Stack of current `this` types for class member bodies.
    pub this_type_stack: Vec<TypeId>,

    /// Current enclosing class info.
    pub enclosing_class: Option<EnclosingClassInfo>,

    /// Type environment for symbol resolution with type parameters.
    /// Used by the evaluator to expand Application types.
    pub type_env: RefCell<TypeEnvironment>,

    // --- DefId Migration Infrastructure ---
    /// Storage for type definitions (interfaces, classes, type aliases).
    /// Part of the DefId migration to decouple Solver from Binder.
    pub definition_store: DefinitionStore,

    /// Mapping from Binder SymbolId to Solver DefId.
    /// Used during migration to avoid creating duplicate DefIds for the same symbol.
    /// Wrapped in RefCell to allow mutation through shared references (for use in Fn closures).
    pub symbol_to_def: RefCell<FxHashMap<SymbolId, DefId>>,

    /// Reverse mapping from Solver DefId to Binder SymbolId.
    /// Used to look up binder symbols from DefId-based types (e.g., namespace exports).
    /// Wrapped in RefCell to allow mutation through shared references (for use in Fn closures).
    pub def_to_symbol: RefCell<FxHashMap<DefId, SymbolId>>,

    /// Type parameters for DefIds (used for type aliases, classes, interfaces).
    /// Enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    /// Wrapped in RefCell to allow mutation through shared references.
    pub def_type_params: RefCell<FxHashMap<DefId, Vec<crate::solver::TypeParamInfo>>>,

    /// Abstract constructor types (TypeIds) produced for abstract classes.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Protected constructor types (TypeIds) produced for protected constructors.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Private constructor types (TypeIds) produced for private constructors.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// All arenas for cross-file resolution (indexed by file_idx from Symbol.decl_file_idx).
    /// Set during multi-file type checking to allow resolving declarations across files.
    pub all_arenas: Option<Vec<Arc<NodeArena>>>,

    /// All binders for cross-file resolution (indexed by file_idx).
    /// Enables looking up exported symbols from other files during import resolution.
    pub all_binders: Option<Vec<Arc<BinderState>>>,

    /// Resolved module paths map: (source_file_idx, specifier) -> target_file_idx.
    /// Used by get_type_of_symbol to resolve imports to their target file and symbol.
    pub resolved_module_paths: Option<FxHashMap<(usize, String), usize>>,

    /// Current file index in multi-file mode (index into all_arenas/all_binders).
    /// Used with resolved_module_paths to look up cross-file imports.
    pub current_file_idx: usize,

    /// Resolved module specifiers for this file (multi-file CLI mode).
    pub resolved_modules: Option<HashSet<String>>,

    /// Per-file cache of is_external_module values to preserve state across files.
    /// Maps file path -> whether that file is an external module (has imports/exports).
    /// This prevents state corruption when binding multiple files sequentially.
    pub is_external_module_by_file: Option<FxHashMap<String, bool>>,

    /// Map of resolution errors: (source_file_idx, specifier) -> Error details.
    /// Populated by the driver when ModuleResolver returns a specific error.
    /// Contains structured error information (code, message) for TS2834, TS2835, TS2792, etc.
    pub resolved_module_errors: Option<FxHashMap<(usize, String), ResolutionError>>,

    /// Import resolution stack for circular import detection.
    /// Tracks the chain of modules being resolved to detect circular dependencies.
    pub import_resolution_stack: Vec<String>,

    /// Symbol resolution depth counter for preventing stack overflow.
    /// Tracks how many nested get_type_of_symbol calls we've made.
    pub symbol_resolution_depth: Cell<u32>,

    /// Maximum symbol resolution depth before we give up (prevents stack overflow).
    pub max_symbol_resolution_depth: u32,

    /// Lib file contexts for global type resolution (lib.es5.d.ts, lib.dom.d.ts, etc.).
    /// Each entry is a (arena, binder) pair from a pre-parsed lib file.
    /// Used as a fallback when resolving type references not found in the main file.
    pub lib_contexts: Vec<LibContext>,

    /// Number of actual lib files loaded (not including user files).
    /// Used by has_lib_loaded() to correctly determine if standard library is available.
    /// This is separate from lib_contexts.len() because lib_contexts may also include
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
    /// Create a new CheckerContext.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        // Create flow graph from the binder's flow nodes
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            report_unresolved_imports: false,
            symbol_types: FxHashMap::default(),
            symbol_instance_types: FxHashMap::default(),
            var_decl_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
            relation_cache: RefCell::new(FxHashMap::default()),
            type_environment: Rc::new(RefCell::new(TypeEnvironment::new())),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            object_spread_property_cache: FxHashMap::default(),
            object_spread_property_set: FxHashSet::default(),
            element_access_type_cache: FxHashMap::default(),
            element_access_type_set: FxHashSet::default(),
            flow_analysis_cache: RefCell::new(FxHashMap::default()),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            symbol_dependencies: FxHashMap::default(),
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: 256,
            class_instance_resolution_set: HashSet::new(),
            class_constructor_resolution_set: HashSet::new(),
            inheritance_graph: crate::solver::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            recursion_depth: Cell::new(0),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: DefinitionStore::new(),
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_to_symbol: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            all_arenas: None,
            all_binders: None,
            resolved_module_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            is_external_module_by_file: None,
            resolved_module_errors: None,
            import_resolution_stack: Vec::new(),
            lib_contexts: Vec::new(),
            actual_lib_file_count: 0,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            type_resolution_fuel: RefCell::new(crate::checker::state::MAX_TYPE_RESOLUTION_OPS),
            fuel_exhausted: RefCell::new(false),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Create a new CheckerContext with explicit compiler options.
    pub fn with_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: &CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.clone().apply_strict_defaults();
        // Create flow graph from the binder's flow nodes
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            report_unresolved_imports: false,
            symbol_types: FxHashMap::default(),
            symbol_instance_types: FxHashMap::default(),
            var_decl_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
            relation_cache: RefCell::new(FxHashMap::default()),
            type_environment: Rc::new(RefCell::new(TypeEnvironment::new())),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            object_spread_property_cache: FxHashMap::default(),
            object_spread_property_set: FxHashSet::default(),
            element_access_type_cache: FxHashMap::default(),
            element_access_type_set: FxHashSet::default(),
            flow_analysis_cache: RefCell::new(FxHashMap::default()),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            symbol_dependencies: FxHashMap::default(),
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: 256,
            class_instance_resolution_set: HashSet::new(),
            class_constructor_resolution_set: HashSet::new(),
            inheritance_graph: crate::solver::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            recursion_depth: Cell::new(0),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: DefinitionStore::new(),
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_to_symbol: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            all_arenas: None,
            all_binders: None,
            resolved_module_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            is_external_module_by_file: None,
            resolved_module_errors: None,
            import_resolution_stack: Vec::new(),
            lib_contexts: Vec::new(),
            actual_lib_file_count: 0,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            type_resolution_fuel: RefCell::new(crate::checker::state::MAX_TYPE_RESOLUTION_OPS),
            fuel_exhausted: RefCell::new(false),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Create a new CheckerContext with a persistent cache.
    /// This allows reusing type checking results from previous queries.
    pub fn with_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        // Create flow graph from the binder's flow nodes
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            report_unresolved_imports: false,
            symbol_types: cache.symbol_types,
            symbol_instance_types: cache.symbol_instance_types,
            var_decl_types: FxHashMap::default(),
            node_types: cache.node_types,
            relation_cache: RefCell::new(cache.relation_cache),
            type_environment: Rc::new(RefCell::new(TypeEnvironment::new())),
            // Use specialized caches from TypeCache to fix Cache Isolation Bug
            application_eval_cache: cache.application_eval_cache,
            application_eval_set: cache.application_eval_set,
            mapped_eval_cache: cache.mapped_eval_cache,
            mapped_eval_set: cache.mapped_eval_set,
            object_spread_property_cache: cache.object_spread_property_cache,
            object_spread_property_set: cache.object_spread_property_set,
            element_access_type_cache: cache.element_access_type_cache,
            element_access_type_set: cache.element_access_type_set,
            flow_analysis_cache: RefCell::new(cache.flow_analysis_cache),
            class_instance_type_to_decl: cache.class_instance_type_to_decl,
            class_instance_type_cache: cache.class_instance_type_cache,
            symbol_dependencies: cache.symbol_dependencies,
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: 256,
            class_instance_resolution_set: HashSet::new(),
            class_constructor_resolution_set: HashSet::new(),
            inheritance_graph: crate::solver::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            recursion_depth: Cell::new(0),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: DefinitionStore::new(),
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            abstract_constructor_types: cache.abstract_constructor_types,
            protected_constructor_types: cache.protected_constructor_types,
            private_constructor_types: cache.private_constructor_types,
            def_to_symbol: RefCell::new(cache.def_to_symbol),
            all_arenas: None,
            all_binders: None,
            resolved_module_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            is_external_module_by_file: None,
            resolved_module_errors: None,
            import_resolution_stack: Vec::new(),
            lib_contexts: Vec::new(),
            actual_lib_file_count: 0,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            type_resolution_fuel: RefCell::new(crate::checker::state::MAX_TYPE_RESOLUTION_OPS),
            fuel_exhausted: RefCell::new(false),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Create a new CheckerContext with explicit compiler options and a persistent cache.
    pub fn with_cache_and_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: &CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.clone().apply_strict_defaults();
        // Create flow graph from the binder's flow nodes
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            report_unresolved_imports: false,
            symbol_types: cache.symbol_types,
            symbol_instance_types: cache.symbol_instance_types,
            var_decl_types: FxHashMap::default(),
            node_types: cache.node_types,
            relation_cache: RefCell::new(cache.relation_cache),
            type_environment: Rc::new(RefCell::new(TypeEnvironment::new())),
            // Use specialized caches from TypeCache to fix Cache Isolation Bug
            application_eval_cache: cache.application_eval_cache,
            application_eval_set: cache.application_eval_set,
            mapped_eval_cache: cache.mapped_eval_cache,
            mapped_eval_set: cache.mapped_eval_set,
            object_spread_property_cache: cache.object_spread_property_cache,
            object_spread_property_set: cache.object_spread_property_set,
            element_access_type_cache: cache.element_access_type_cache,
            element_access_type_set: cache.element_access_type_set,
            flow_analysis_cache: RefCell::new(cache.flow_analysis_cache),
            class_instance_type_to_decl: cache.class_instance_type_to_decl,
            class_instance_type_cache: cache.class_instance_type_cache,
            symbol_dependencies: cache.symbol_dependencies,
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: 256,
            class_instance_resolution_set: HashSet::new(),
            class_constructor_resolution_set: HashSet::new(),
            inheritance_graph: crate::solver::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            recursion_depth: Cell::new(0),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: DefinitionStore::new(),
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            abstract_constructor_types: cache.abstract_constructor_types,
            protected_constructor_types: cache.protected_constructor_types,
            private_constructor_types: cache.private_constructor_types,
            def_to_symbol: RefCell::new(cache.def_to_symbol),
            all_arenas: None,
            all_binders: None,
            resolved_module_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            is_external_module_by_file: None,
            resolved_module_errors: None,
            import_resolution_stack: Vec::new(),
            lib_contexts: Vec::new(),
            actual_lib_file_count: 0,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            type_resolution_fuel: RefCell::new(crate::checker::state::MAX_TYPE_RESOLUTION_OPS),
            fuel_exhausted: RefCell::new(false),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Create a child CheckerContext that shares the parent's caches.
    /// This is used for temporary checkers (e.g., cross-file symbol resolution)
    /// to ensure cache results are not lost (fixes Cache Isolation Bug).
    ///
    /// The child context shares the parent's caches through Rc<RefCell<>> wrappers,
    /// allowing both contexts to read and write to the same cache.
    pub fn with_parent_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &CheckerContext<'a>,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        // Share caches through Rc<RefCell<>> to allow both parent and child to access
        use std::cell::RefCell;
        use std::rc::Rc;

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            report_unresolved_imports: false,
            symbol_types: parent.symbol_types.clone(),
            symbol_instance_types: parent.symbol_instance_types.clone(),
            var_decl_types: FxHashMap::default(),
            node_types: parent.node_types.clone(),
            relation_cache: parent.relation_cache.clone(),
            type_environment: Rc::new(RefCell::new(TypeEnvironment::new())),
            // Share specialized caches from parent
            application_eval_cache: parent.application_eval_cache.clone(),
            application_eval_set: parent.application_eval_set.clone(),
            mapped_eval_cache: parent.mapped_eval_cache.clone(),
            mapped_eval_set: parent.mapped_eval_set.clone(),
            object_spread_property_cache: parent.object_spread_property_cache.clone(),
            object_spread_property_set: parent.object_spread_property_set.clone(),
            element_access_type_cache: parent.element_access_type_cache.clone(),
            element_access_type_set: parent.element_access_type_set.clone(),
            flow_analysis_cache: parent.flow_analysis_cache.clone(),
            class_instance_type_to_decl: parent.class_instance_type_to_decl.clone(),
            class_instance_type_cache: parent.class_instance_type_cache.clone(),
            symbol_dependencies: parent.symbol_dependencies.clone(),
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: 256,
            class_instance_resolution_set: HashSet::new(),
            class_constructor_resolution_set: HashSet::new(),
            inheritance_graph: crate::solver::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            recursion_depth: Cell::new(0),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: DefinitionStore::new(),
            symbol_to_def: parent.symbol_to_def.clone(),
            def_type_params: parent.def_type_params.clone(),
            abstract_constructor_types: parent.abstract_constructor_types.clone(),
            protected_constructor_types: parent.protected_constructor_types.clone(),
            private_constructor_types: parent.private_constructor_types.clone(),
            def_to_symbol: parent.def_to_symbol.clone(),
            all_arenas: None,
            all_binders: None,
            resolved_module_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            is_external_module_by_file: None,
            resolved_module_errors: None,
            import_resolution_stack: Vec::new(),
            lib_contexts: Vec::new(),
            actual_lib_file_count: 0,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            type_resolution_fuel: RefCell::new(crate::checker::state::MAX_TYPE_RESOLUTION_OPS),
            fuel_exhausted: RefCell::new(false),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Set lib contexts for global type resolution.
    /// Note: lib_contexts may include both actual lib files AND user files for cross-file
    /// resolution. Use set_actual_lib_file_count() to track how many are actual lib files.
    pub fn set_lib_contexts(&mut self, lib_contexts: Vec<LibContext>) {
        self.lib_contexts = lib_contexts;
    }

    /// Set the count of actual lib files loaded (not including user files).
    /// This is used by has_lib_loaded() to correctly determine if standard library is available.
    pub fn set_actual_lib_file_count(&mut self, count: usize) {
        self.actual_lib_file_count = count;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Vec<Arc<NodeArena>>) {
        self.all_arenas = Some(arenas);
    }

    /// Set all binders for cross-file resolution.
    pub fn set_all_binders(&mut self, binders: Vec<Arc<BinderState>>) {
        self.all_binders = Some(binders);
    }

    /// Set resolved module paths map for cross-file import resolution.
    pub fn set_resolved_module_paths(&mut self, paths: FxHashMap<(usize, String), usize>) {
        self.resolved_module_paths = Some(paths);
    }

    /// Set resolved module specifiers (module names that exist in the project).
    /// Used to suppress TS2307 errors for known modules.
    pub fn set_resolved_modules(&mut self, modules: HashSet<String>) {
        self.resolved_modules = Some(modules);
    }

    /// Set resolved module errors map for cross-file import resolution.
    /// Populated by the driver when ModuleResolver returns specific errors (TS2834, TS2835, TS2792, etc.).
    pub fn set_resolved_module_errors(
        &mut self,
        errors: FxHashMap<(usize, String), ResolutionError>,
    ) {
        self.resolved_module_errors = Some(errors);
    }

    /// Get the resolution error for a specifier, if any.
    /// Returns the specific error (TS2834, TS2835, TS2792, etc.) if the module resolution failed with a known error.
    pub fn get_resolution_error(&self, specifier: &str) -> Option<&ResolutionError> {
        self.resolved_module_errors
            .as_ref()
            .and_then(|errors| errors.get(&(self.current_file_idx, specifier.to_string())))
    }

    /// Set the current file index.
    pub fn set_current_file_idx(&mut self, idx: usize) {
        self.current_file_idx = idx;
    }

    /// Get the arena for a specific file index.
    /// Returns the current arena if file_idx is u32::MAX (single-file mode).
    pub fn get_arena_for_file(&self, file_idx: u32) -> &NodeArena {
        if file_idx == u32::MAX {
            return self.arena;
        }
        if let Some(ref arenas) = self.all_arenas
            && let Some(arena) = arenas.get(file_idx as usize)
        {
            return arena.as_ref();
        }
        self.arena
    }

    /// Get the binder for a specific file index.
    /// Returns None if file_idx is out of bounds or all_binders is not set.
    pub fn get_binder_for_file(&self, file_idx: usize) -> Option<&BinderState> {
        self.all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .map(Arc::as_ref)
    }

    /// Resolve an import specifier to its target file index.
    /// Uses the resolved_module_paths map populated by the driver.
    /// Returns None if the import cannot be resolved (e.g., external module).
    pub fn resolve_import_target(&self, specifier: &str) -> Option<usize> {
        self.resolved_module_paths.as_ref().and_then(|paths| {
            paths
                .get(&(self.current_file_idx, specifier.to_string()))
                .copied()
        })
    }

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        TypeCache {
            symbol_types: self.symbol_types,
            symbol_instance_types: self.symbol_instance_types,
            node_types: self.node_types,
            relation_cache: self.relation_cache.into_inner(),
            symbol_dependencies: self.symbol_dependencies,
            abstract_constructor_types: self.abstract_constructor_types,
            protected_constructor_types: self.protected_constructor_types,
            private_constructor_types: self.private_constructor_types,
            def_to_symbol: self.def_to_symbol.into_inner(),
            // Specialized caches
            application_eval_cache: self.application_eval_cache,
            application_eval_set: self.application_eval_set,
            mapped_eval_cache: self.mapped_eval_cache,
            mapped_eval_set: self.mapped_eval_set,
            object_spread_property_cache: self.object_spread_property_cache,
            object_spread_property_set: self.object_spread_property_set,
            element_access_type_cache: self.element_access_type_cache,
            element_access_type_set: self.element_access_type_set,
            flow_analysis_cache: self.flow_analysis_cache.into_inner(),
            class_instance_type_to_decl: self.class_instance_type_to_decl,
            class_instance_type_cache: self.class_instance_type_cache,
        }
    }

    // =========================================================================
    // DefId Migration Helpers
    // =========================================================================

    /// Get or create a DefId for a symbol.
    ///
    /// If the symbol already has a DefId, return it.
    /// Otherwise, create a new DefId and store the mapping.
    ///
    /// This is used during the migration from SymbolRef to DefId.
    /// Eventually, all type references will use DefId directly.
    pub fn get_or_create_def_id(&self, sym_id: SymbolId) -> DefId {
        use crate::solver::def::DefinitionInfo;

        if let Some(&def_id) = self.symbol_to_def.borrow().get(&sym_id) {
            return def_id;
        }

        // Get symbol info to create DefinitionInfo
        let Some(symbol) = self.binder.symbols.get(sym_id) else {
            // Symbol not found - return invalid DefId
            return DefId::INVALID;
        };
        let name = self.types.intern_string(&symbol.escaped_name);

        // Determine DefKind from symbol flags
        let kind = if (symbol.flags & crate::binder::symbol_flags::TYPE_ALIAS) != 0 {
            crate::solver::def::DefKind::TypeAlias
        } else if (symbol.flags & crate::binder::symbol_flags::INTERFACE) != 0 {
            crate::solver::def::DefKind::Interface
        } else if (symbol.flags & crate::binder::symbol_flags::CLASS) != 0 {
            crate::solver::def::DefKind::Class
        } else if (symbol.flags & crate::binder::symbol_flags::ENUM) != 0 {
            crate::solver::def::DefKind::Enum
        } else if (symbol.flags
            & (crate::binder::symbol_flags::NAMESPACE_MODULE
                | crate::binder::symbol_flags::VALUE_MODULE))
            != 0
        {
            crate::solver::def::DefKind::Namespace
        } else {
            // Default to TypeAlias for other symbols
            crate::solver::def::DefKind::TypeAlias
        };

        // Create a placeholder DefinitionInfo - body will be set lazily
        // Get span from the first declaration if available
        let span = symbol.declarations.first().map(|n| (n.0, n.0));

        let info = DefinitionInfo {
            kind,
            name,
            type_params: Vec::new(), // Will be populated when type is resolved
            body: None,              // Lazy: computed on first access
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(), // Will be populated for namespaces/modules
            file_id: Some(symbol.decl_file_idx),
            span,
        };

        let def_id = self.definition_store.register(info);
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

        def_id
    }

    /// Create a Lazy type reference from a symbol.
    ///
    /// This returns `TypeKey::Lazy(DefId)` for use in the new DefId system.
    /// During migration, this is called alongside or instead of creating
    /// `TypeKey::Ref(SymbolRef)`.
    pub fn create_lazy_type_ref(&mut self, sym_id: SymbolId) -> TypeId {
        use crate::solver::TypeKey;

        let def_id = self.get_or_create_def_id(sym_id);
        self.types.intern(TypeKey::Lazy(def_id))
    }

    /// Convert TypeKey::Ref to TypeKey::Lazy(DefId) if needed (Phase 1 migration).
    ///
    /// This post-processes a TypeId created by TypeLowering. If the type is
    /// TypeKey::Ref(SymbolRef), this creates a corresponding TypeKey::Lazy(DefId)
    /// for the same symbol. This enables gradual migration from SymbolRef to DefId.
    ///
    /// **Pattern:** TypeLowering creates Ref → post-process → returns Lazy
    ///
    /// PHASE 4.2: TypeKey::Ref removed, all types are now Lazy(DefId).
    /// This function is now a no-op since all types are already Lazy.
    pub fn maybe_create_lazy_from_resolved(&mut self, type_id: TypeId) -> TypeId {
        type_id
    }

    /// Look up the SymbolId for a DefId (reverse mapping).
    pub fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol.borrow().get(&def_id).copied()
    }

    /// Insert type parameters for a DefId (Phase 4.2.1: generic type alias support).
    ///
    /// This enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    ///
    /// # Example
    /// ```ignore
    /// // For type List<T> = { value: T; next: List<T> | null }
    /// let def_id = ctx.get_or_create_def_id(list_sym_id);
    /// let params = vec![TypeParamInfo { name: "T", ... }];
    /// ctx.insert_def_type_params(def_id, params);
    /// ```
    pub fn insert_def_type_params(&self, def_id: DefId, params: Vec<crate::solver::TypeParamInfo>) {
        if !params.is_empty() {
            self.def_type_params.borrow_mut().insert(def_id, params);
        }
    }

    /// Get type parameters for a DefId.
    ///
    /// Returns None if the DefId has no type parameters or hasn't been registered yet.
    pub fn get_def_type_params(&self, def_id: DefId) -> Option<Vec<crate::solver::TypeParamInfo>> {
        self.def_type_params.borrow().get(&def_id).cloned()
    }

    /// Resolve a TypeId to its underlying SymbolId if it is a reference type.
    ///
    /// This helper bridges the DefId-based Solver and SymbolId-based Binder.
    /// It handles the indirection automatically: TypeId → DefId → SymbolId.
    ///
    /// # Example
    /// ```ignore
    /// // Old (broken):
    /// if let Some(sym_ref) = get_ref_symbol(self.ctx.types, type_id) {
    ///     let sym_id = SymbolId(sym_ref.0); // BROKEN CAST
    /// }
    ///
    /// // New (correct):
    /// if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) {
    ///     // use sym_id
    /// }
    /// ```
    pub fn resolve_type_to_symbol_id(&self, type_id: TypeId) -> Option<SymbolId> {
        use crate::solver::type_queries;

        // 1. Try to get DefId from Lazy type - Phase 4.2+
        if let Some(def_id) = type_queries::get_lazy_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 2. Try to get DefId from Enum type
        if let Some(def_id) = type_queries::get_enum_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 3. Fallback for legacy Ref types (if any remain during migration)
        #[allow(deprecated)]
        if let Some(sym_ref) = type_queries::get_symbol_ref(self.types, type_id) {
            return Some(SymbolId(sym_ref.0));
        }

        None
    }

    /// Look up an existing DefId for a symbol without creating a new one.
    ///
    /// Returns None if the symbol doesn't have a DefId yet.
    /// This is used by the DefId resolver in TypeLowering to prefer
    /// DefId when available but fall back to SymbolRef otherwise.
    pub fn get_existing_def_id(&self, sym_id: SymbolId) -> Option<DefId> {
        self.symbol_to_def.borrow().get(&sym_id).copied()
    }

    /// Create a TypeFormatter with full context for displaying types (Phase 4.2.1).
    ///
    /// This includes symbol arena and definition store, which allows the formatter
    /// to display type names for Lazy(DefId) types instead of the internal "Lazy(def_id)"
    /// representation.
    ///
    /// # Example
    /// ```ignore
    /// let formatter = self.create_type_formatter();
    /// let type_str = formatter.format(type_id);  // Shows "List<number>" not "Lazy(1)<number>"
    /// ```
    pub fn create_type_formatter(&self) -> crate::solver::TypeFormatter<'_> {
        use crate::solver::TypeFormatter;

        TypeFormatter::with_symbols(self.types, &self.binder.symbols)
            .with_def_store(&self.definition_store)
    }

    /// Register a resolved type in the TypeEnvironment for both SymbolRef and DefId.
    ///
    /// This ensures that both the old `TypeKey::Ref(SymbolRef)` and new `TypeKey::Lazy(DefId)`
    /// paths can resolve the type during evaluation.
    ///
    /// Should be called when a symbol's type is resolved via `get_type_of_symbol`.
    pub fn register_resolved_type(
        &mut self,
        sym_id: SymbolId,
        type_id: TypeId,
        type_params: Vec<crate::solver::TypeParamInfo>,
    ) {
        use crate::solver::SymbolRef;

        // Try to borrow mutably - skip if already borrowed (during recursive resolution)
        if let Ok(mut env) = self.type_environment.try_borrow_mut() {
            // Insert with SymbolRef key (existing path)
            if type_params.is_empty() {
                env.insert(SymbolRef(sym_id.0), type_id);
            } else {
                env.insert_with_params(SymbolRef(sym_id.0), type_id, type_params.clone());
            }

            // Also insert with DefId key if one exists (Phase 4.3 migration)
            if let Some(&def_id) = self.symbol_to_def.borrow().get(&sym_id) {
                if type_params.is_empty() {
                    env.insert_def(def_id, type_id);
                } else {
                    env.insert_def_with_params(def_id, type_id, type_params);
                }

                // Register mapping for InheritanceGraph bridge (Phase 3.2)
                // This enables Lazy(DefId) types to use the O(1) InheritanceGraph
                env.register_def_symbol_mapping(def_id, sym_id);
            }
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

    /// Enter an async context (increment async depth).
    pub fn enter_async_context(&mut self) {
        self.async_depth += 1;
    }

    /// Exit an async context (decrement async depth).
    pub fn exit_async_context(&mut self) {
        if self.async_depth > 0 {
            self.async_depth -= 1;
        }
    }

    /// Check if we're currently inside an async function.
    pub fn in_async_context(&self) -> bool {
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
    /// Prevents stack overflow by bailing out when depth exceeds MAX_CHECKER_RECURSION_DEPTH.
    #[inline]
    pub fn enter_recursion(&self) -> bool {
        let depth = self.recursion_depth.get();
        if depth >= crate::limits::MAX_CHECKER_RECURSION_DEPTH {
            return false;
        }
        self.recursion_depth.set(depth + 1);
        true
    }

    /// Leave a recursive call (decrement depth counter).
    #[inline]
    pub fn leave_recursion(&self) {
        let depth = self.recursion_depth.get();
        debug_assert!(
            depth > 0,
            "leave_recursion called without matching enter_recursion"
        );
        self.recursion_depth.set(depth - 1);
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
    /// This is a generalized version of has_symbol_in_lib for any symbol name.
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
        use crate::lib_loader;

        // ES2015+ types
        if lib_loader::is_es2015_plus_type(name) {
            return true;
        }

        // Pre-ES2015 global types that are commonly used
        // These are always available in lib.d.ts but should emit TS2318 when @noLib is enabled
        match name {
            "Object" | "Function" | "Array" | "String" | "Number" | "Boolean" | "Date"
            | "RegExp" | "Error" | "Math" | "JSON" | "console" | "window" | "document"
            | "ArrayBuffer" | "DataView" | "Int8Array" | "Uint8Array" | "Uint8ClampedArray"
            | "Int16Array" | "Uint16Array" | "Int32Array" | "Uint32Array" | "Float32Array"
            | "Float64Array" | "this" | "globalThis" | "IArguments" => true,
            _ => false,
        }
    }

    /// Check if a global type is missing due to insufficient ES version support.
    /// Returns the minimum ES version required for this type, or None if not applicable.
    pub fn get_required_es_version_for_global(&self, name: &str) -> Option<&'static str> {
        use crate::lib_loader;

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
    pub fn has_modifier(&self, modifiers: &Option<crate::parser::NodeList>, kind: u16) -> bool {
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
    pub fn check_flow_usage(&self, node_idx: NodeIndex) -> Option<crate::binder::FlowNodeId> {
        if let Some(ref _graph) = self.flow_graph {
            // Look up the flow node for this AST node from the binder's node_flow mapping
            self.binder.node_flow.get(&node_idx.0).copied()
        } else {
            None
        }
    }

    /// Get a reference to the flow graph.
    pub fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }

    // =========================================================================
    // Compiler Option Accessors
    // =========================================================================

    /// Check if strict mode is enabled.
    pub fn is_strict_mode(&self) -> bool {
        self.compiler_options.strict
    }

    /// Check if noImplicitAny is enabled.
    pub fn no_implicit_any(&self) -> bool {
        self.compiler_options.no_implicit_any
    }

    /// Check if noImplicitReturns is enabled.
    pub fn no_implicit_returns(&self) -> bool {
        self.compiler_options.no_implicit_returns
    }

    /// Check if noImplicitThis is enabled.
    pub fn no_implicit_this(&self) -> bool {
        self.compiler_options.no_implicit_this
    }

    /// Check if strictNullChecks is enabled.
    pub fn strict_null_checks(&self) -> bool {
        self.compiler_options.strict_null_checks
    }

    /// Check if strictFunctionTypes is enabled.
    pub fn strict_function_types(&self) -> bool {
        self.compiler_options.strict_function_types
    }

    /// Check if strictPropertyInitialization is enabled.
    pub fn strict_property_initialization(&self) -> bool {
        self.compiler_options.strict_property_initialization
    }

    /// Check if useUnknownInCatchVariables is enabled.
    pub fn use_unknown_in_catch_variables(&self) -> bool {
        self.compiler_options.use_unknown_in_catch_variables
    }

    /// Check if isolatedModules is enabled.
    pub fn isolated_modules(&self) -> bool {
        self.compiler_options.isolated_modules
    }

    /// Check if noUncheckedIndexedAccess is enabled.
    /// When enabled, index signature access adds `| undefined` to the result type.
    pub fn no_unchecked_indexed_access(&self) -> bool {
        self.compiler_options.no_unchecked_indexed_access
    }

    /// Check if strictBindCallApply is enabled.
    /// When enabled, bind/call/apply use strict function signatures.
    pub fn strict_bind_call_apply(&self) -> bool {
        self.compiler_options.strict_bind_call_apply
    }

    /// Check if exactOptionalPropertyTypes is enabled.
    /// When enabled, optional properties are `T | undefined` not `T | undefined | missing`.
    pub fn exact_optional_property_types(&self) -> bool {
        self.compiler_options.exact_optional_property_types
    }

    /// Convert CheckerOptions to JudgeConfig for the CompatChecker.
    fn as_judge_config(&self) -> JudgeConfig {
        JudgeConfig {
            strict_function_types: self.strict_function_types(),
            strict_null_checks: self.strict_null_checks(),
            exact_optional_property_types: self.exact_optional_property_types(),
            no_unchecked_indexed_access: self.no_unchecked_indexed_access(),
        }
    }

    /// Apply standard compiler options to a CompatChecker, including query_db.
    /// This wires the CompilerOptions (via JudgeConfig) and the QueryDatabase.
    pub fn configure_compat_checker<R: crate::solver::TypeResolver>(
        &self,
        checker: &mut crate::solver::CompatChecker<'a, R>,
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
    pub fn no_unused_locals(&self) -> bool {
        self.compiler_options.no_unused_locals
    }

    /// Check if noUnusedParameters is enabled.
    pub fn no_unused_parameters(&self) -> bool {
        self.compiler_options.no_unused_parameters
    }

    /// Check if noLib is enabled.
    /// When enabled, no library files (including lib.d.ts) are included.
    /// TS2318 errors are emitted when referencing global types with this option enabled.
    pub fn no_lib(&self) -> bool {
        self.compiler_options.no_lib
    }

    /// Check if lib files are loaded (lib.d.ts, etc.).
    /// Returns false when noLib is enabled or when no actual lib files are loaded.
    /// Uses actual_lib_file_count instead of lib_contexts.is_empty() because lib_contexts
    /// may also contain user file contexts for cross-file resolution in multi-file tests.
    /// Used to determine whether to emit TS2304/TS2318/TS2583 for missing global types.
    pub fn has_lib_loaded(&self) -> bool {
        !self.compiler_options.no_lib && self.actual_lib_file_count > 0
    }

    /// Check if esModuleInterop is enabled.
    /// When enabled, synthesizes default exports for CommonJS modules.
    pub fn es_module_interop(&self) -> bool {
        self.compiler_options.es_module_interop
    }

    /// Check if allowSyntheticDefaultImports is enabled.
    /// When enabled, allows `import x from 'y'` when module doesn't have default export.
    /// This is implied by esModuleInterop.
    pub fn allow_synthetic_default_imports(&self) -> bool {
        self.compiler_options.allow_synthetic_default_imports
    }
}

// =============================================================================
// TypeResolver Implementation for Lazy Type Resolution
// =============================================================================

/// Implement TypeResolver for CheckerContext to support Lazy type resolution.
///
/// This enables ApplicationEvaluator to resolve TypeKey::Lazy(DefId) references
/// by looking up the cached type for a symbol. The cache is populated during
/// type checking when get_type_of_symbol() is called.
///
/// **Architecture Note:**
/// - resolve_lazy() is read-only (looks up from cache)
/// - Cache is populated by CheckerState::get_type_of_symbol() before Application evaluation
/// - This separation keeps the solver layer (ApplicationEvaluator) independent of checker logic
impl<'a> crate::solver::TypeResolver for CheckerContext<'a> {
    /// Resolve a symbol reference to its cached type (deprecated).
    ///
    /// Phase 4.2: TypeKey::Ref is removed, but we keep this for compatibility.
    /// Converts SymbolRef to SymbolId and looks up in cache.
    #[allow(deprecated)]
    fn resolve_ref(
        &self,
        symbol: crate::solver::types::SymbolRef,
        _interner: &dyn crate::solver::TypeDatabase,
    ) -> Option<crate::solver::TypeId> {
        let sym_id = crate::binder::SymbolId(symbol.0);
        self.symbol_types.get(&sym_id).copied()
    }

    /// Resolve a DefId to its cached type.
    ///
    /// This looks up the type from the symbol_types cache, which is populated
    /// during type checking. Returns None if the symbol hasn't been resolved yet.
    ///
    /// **Callers should ensure get_type_of_symbol() is called first** to populate
    /// the cache before calling resolve_lazy().
    fn resolve_lazy(
        &self,
        def_id: crate::solver::DefId,
        _interner: &dyn crate::solver::TypeDatabase,
    ) -> Option<crate::solver::TypeId> {
        use crate::binder::symbol_flags;

        // Convert DefId to SymbolId using the reverse mapping
        if let Some(sym_id) = self.def_to_symbol_id(def_id) {
            // For classes, check if we should return instance type instead of constructor type
            if let Some(symbol) = self.binder.symbols.get(sym_id) {
                // Check if symbol is a class
                if (symbol.flags & symbol_flags::CLASS) != 0 {
                    // For classes in TYPE position, return instance type
                    if let Some(instance_type) = self.symbol_instance_types.get(&sym_id) {
                        return Some(*instance_type);
                    }
                }
            }

            // Look up the cached type for this symbol (constructor type for classes)
            self.symbol_types.get(&sym_id).copied()
        } else {
            None
        }
    }

    /// Get type parameters for a symbol reference (deprecated).
    ///
    /// Type parameters are embedded in the type itself rather than stored separately.
    #[allow(deprecated)]
    fn get_type_params(
        &self,
        _symbol: crate::solver::types::SymbolRef,
    ) -> Option<Vec<crate::solver::TypeParamInfo>> {
        None
    }

    /// Get type parameters for a Lazy type.
    ///
    /// Phase 4.2.1: For type aliases, type parameters are stored in def_type_params
    /// and used by the Solver to expand Application(Lazy(DefId), Args).
    ///
    /// For classes/interfaces, type parameters are embedded in the resolved type's shape
    /// (Callable.type_params, Interface.type_params, etc.) rather than stored separately.
    fn get_lazy_type_params(
        &self,
        def_id: crate::solver::DefId,
    ) -> Option<Vec<crate::solver::TypeParamInfo>> {
        // Phase 4.2.1: Look up type parameters for type aliases
        self.get_def_type_params(def_id)
    }

    /// Get the base class type for a class/interface type.
    ///
    /// This implements the TypeResolver trait method for Best Common Type (BCT) algorithm.
    /// For example, given Dog that extends Animal, this returns the type for Animal.
    ///
    /// **Architecture**: Bridges Solver (BCT computation) to Binder (extends clauses) via:
    /// 1. TypeId -> DefId (from Lazy type)
    /// 2. DefId -> SymbolId (via def_to_symbol mapping)
    /// 3. SymbolId -> Parent SymbolId (via InheritanceGraph)
    /// 4. Parent SymbolId -> TypeId (via symbol_types cache)
    ///
    /// Returns None if:
    /// - The type is not a Lazy type (not a class/interface)
    /// - The DefId has no corresponding SymbolId
    /// - The class has no base class (no parents in InheritanceGraph)
    fn get_base_type(
        &self,
        type_id: crate::solver::TypeId,
        interner: &dyn crate::solver::TypeDatabase,
    ) -> Option<crate::solver::TypeId> {
        
        use crate::solver::type_queries;
        use crate::solver::visitor::{
            callable_shape_id, object_shape_id, object_with_index_shape_id,
        };

        // 1. First try Lazy types (type aliases, class/interface references)
        if let Some(def_id) = type_queries::get_lazy_def_id(self.types, type_id) {
            // 2. Convert DefId to SymbolId
            let sym_id = self.def_to_symbol_id(def_id)?;

            // 3. Get parents from InheritanceGraph (populated during class/interface binding)
            // Works for both classes (single inheritance) and interfaces (multiple extends)
            let parents = self.inheritance_graph.get_parents(sym_id);

            // 4. Return the first parent's type (the immediate base class/interface)
            // Note: For interfaces with multiple parents, we only return the first one.
            // This is sufficient for BCT which checks all candidates in the set.
            if let Some(parent_sym_id) = parents.first() {
                // Look up the cached type for the parent symbol
                // For classes, we need the instance type, not constructor type
                if let Some(instance_type) = self.symbol_instance_types.get(parent_sym_id) {
                    return Some(*instance_type);
                }
                // Fallback to symbol_types (constructor type) if instance type not available
                return self.symbol_types.get(parent_sym_id).copied();
            }
            return None;
        }

        // 2. For class instance types (ObjectWithIndex types), check the ObjectShape symbol
        if let Some(shape_id) = object_shape_id(interner, type_id)
            .or_else(|| object_with_index_shape_id(interner, type_id))
        {
            let shape = interner.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol {
                // Use InheritanceGraph to get parent
                let parents = self.inheritance_graph.get_parents(sym_id);
                if let Some(&parent_sym_id) = parents.first() {
                    // For classes, try instance_types first; for interfaces, use symbol_types
                    if let Some(instance_type) = self.symbol_instance_types.get(&parent_sym_id) {
                        return Some(*instance_type);
                    }
                    // Fallback to symbol_types (for interfaces)
                    return self.symbol_types.get(&parent_sym_id).copied();
                }
            }
        }

        // 3. For class instance types (Callable types), get the class declaration and check InheritanceGraph
        if let Some(_shape_id) = callable_shape_id(interner, type_id) {
            // Step 1: TypeId -> NodeIndex (Class Declaration)
            if let Some(&decl_idx) = self.class_instance_type_to_decl.get(&type_id) {
                // Step 2: NodeIndex -> SymbolId (Class Symbol)
                // This is the correct way to get the symbol without scope/name lookup issues
                if let Some(sym_id) = self.binder.get_node_symbol(decl_idx) {
                    // Step 3: SymbolId -> Parent SymbolId (via InheritanceGraph)
                    let parents = self.inheritance_graph.get_parents(sym_id);
                    if let Some(&parent_sym_id) = parents.first() {
                        // Step 4: Parent SymbolId -> Parent TypeId (Instance Type)
                        if let Some(instance_type) = self.symbol_instance_types.get(&parent_sym_id)
                        {
                            return Some(*instance_type);
                        }
                    }
                }
            }
        }

        None
    }
}
