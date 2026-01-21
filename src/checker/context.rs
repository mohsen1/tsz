//! Checker Context
//!
//! Holds the shared state used throughout the type checking process.
//! This separates state from logic, allowing specialized checkers (expressions, statements)
//! to borrow the context mutably.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::binder::SymbolId;
use crate::checker::control_flow::FlowGraph;
use crate::checker::types::diagnostics::Diagnostic;
use crate::parser::NodeIndex;

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
use crate::solver::{TypeEnvironment, TypeId, TypeInterner};

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
}

/// Persistent cache for type checking results across LSP queries.
/// This cache survives between LSP requests but is invalidated when the file changes.
#[derive(Clone, Debug, Default)]
pub struct TypeCache {
    /// Cached types for symbols.
    pub symbol_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Type parameter names for type_to_string.
    pub type_parameter_names: FxHashMap<TypeId, String>,

    /// Cache for type relation results (subtype checking).
    pub relation_cache: FxHashMap<(TypeId, TypeId, u8), bool>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Cached abstract constructor types (TypeIds) for assignability checks.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Cached protected constructor types (TypeIds) for assignability checks.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Cached private constructor types (TypeIds) for assignability checks.
    pub private_constructor_types: FxHashSet<TypeId>,
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
            self.symbol_dependencies.remove(sym_id);
        }
        self.node_types.clear();
        self.abstract_constructor_types.clear();
        self.protected_constructor_types.clear();
        self.private_constructor_types.clear();

        affected.len()
    }
}

/// Shared state for type checking.
pub struct CheckerContext<'a> {
    /// The NodeArena containing the AST.
    pub arena: &'a NodeArena,

    /// The binder state with symbols.
    pub binder: &'a BinderState,

    /// Type interner for structural type interning.
    pub types: &'a TypeInterner,

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

    /// Cached types for variable declarations (used for TS2403 checks).
    pub var_decl_types: FxHashMap<SymbolId, TypeId>,

    /// Cached types for nodes.
    pub node_types: FxHashMap<u32, TypeId>,

    /// Type parameter names for type_to_string.
    pub type_parameter_names: FxHashMap<TypeId, String>,

    /// Cache for type relation results.
    pub relation_cache: RefCell<FxHashMap<(TypeId, TypeId, u8), bool>>,

    /// Cached type environment for resolving Ref types during assignability checks.
    pub type_environment: RefCell<Option<TypeEnvironment>>,

    /// Cache for evaluated application types to avoid repeated expansion.
    pub application_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for application evaluation.
    pub application_eval_set: FxHashSet<TypeId>,

    /// Cache for evaluated mapped types with symbol resolution.
    pub mapped_eval_cache: FxHashMap<TypeId, TypeId>,

    /// Recursion guard for mapped type evaluation with resolution.
    pub mapped_eval_set: FxHashSet<TypeId>,

    /// Symbol dependency graph (symbol -> referenced symbols).
    pub symbol_dependencies: FxHashMap<SymbolId, FxHashSet<SymbolId>>,

    /// Stack of symbols currently being evaluated for dependency tracking.
    pub symbol_dependency_stack: Vec<SymbolId>,

    // --- Diagnostics ---
    /// Diagnostics produced during type checking.
    pub diagnostics: Vec<Diagnostic>,

    // --- Recursion Guards ---
    /// Stack of symbols being resolved.
    pub symbol_resolution_stack: Vec<SymbolId>,
    /// O(1) lookup set for symbol resolution stack.
    pub symbol_resolution_set: HashSet<SymbolId>,
    /// O(1) lookup set for class instance type resolution to avoid recursion.
    pub class_instance_resolution_set: HashSet<SymbolId>,

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

    /// Abstract constructor types (TypeIds) produced for abstract classes.
    pub abstract_constructor_types: FxHashSet<TypeId>,

    /// Protected constructor types (TypeIds) produced for protected constructors.
    pub protected_constructor_types: FxHashSet<TypeId>,

    /// Private constructor types (TypeIds) produced for private constructors.
    pub private_constructor_types: FxHashSet<TypeId>,

    /// All arenas for cross-file resolution (indexed by file_idx from Symbol.decl_file_idx).
    /// Set during multi-file type checking to allow resolving declarations across files.
    pub all_arenas: Option<Vec<Arc<NodeArena>>>,

    /// Resolved module specifiers for this file (multi-file CLI mode).
    pub resolved_modules: Option<HashSet<String>>,

    /// Lib file contexts for global type resolution (lib.es5.d.ts, lib.dom.d.ts, etc.).
    /// Each entry is a (arena, binder) pair from a pre-parsed lib file.
    /// Used as a fallback when resolving type references not found in the main file.
    pub lib_contexts: Vec<LibContext>,

    /// Control flow graph for definite assignment analysis and type narrowing.
    /// This is built during the binding phase and used by the checker.
    pub flow_graph: Option<FlowGraph<'a>>,

    /// Async context depth - tracks nesting of async functions.
    /// Used to check if await expressions are within async context (TS1359).
    pub async_depth: u32,
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
        types: &'a TypeInterner,
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
            report_unresolved_imports: true,
            symbol_types: FxHashMap::default(),
            var_decl_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
            type_parameter_names: FxHashMap::default(),
            relation_cache: RefCell::new(FxHashMap::default()),
            type_environment: RefCell::new(None),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            symbol_dependencies: FxHashMap::default(),
            symbol_dependency_stack: Vec::new(),
            diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            class_instance_resolution_set: HashSet::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            all_arenas: None,
            resolved_modules: None,
            lib_contexts: Vec::new(),
            flow_graph,
            async_depth: 0,
        }
    }

    /// Create a new CheckerContext with explicit compiler options.
    pub fn with_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
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
            report_unresolved_imports: true,
            symbol_types: FxHashMap::default(),
            var_decl_types: FxHashMap::default(),
            node_types: FxHashMap::default(),
            type_parameter_names: FxHashMap::default(),
            relation_cache: RefCell::new(FxHashMap::default()),
            type_environment: RefCell::new(None),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            symbol_dependencies: FxHashMap::default(),
            symbol_dependency_stack: Vec::new(),
            diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            class_instance_resolution_set: HashSet::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            all_arenas: None,
            resolved_modules: None,
            lib_contexts: Vec::new(),
            flow_graph,
            async_depth: 0,
        }
    }

    /// Create a new CheckerContext with a persistent cache.
    /// This allows reusing type checking results from previous queries.
    pub fn with_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
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
            report_unresolved_imports: true,
            symbol_types: cache.symbol_types,
            var_decl_types: FxHashMap::default(),
            node_types: cache.node_types,
            type_parameter_names: cache.type_parameter_names,
            relation_cache: RefCell::new(cache.relation_cache),
            type_environment: RefCell::new(None),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            symbol_dependencies: cache.symbol_dependencies,
            symbol_dependency_stack: Vec::new(),
            diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            class_instance_resolution_set: HashSet::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            abstract_constructor_types: cache.abstract_constructor_types,
            protected_constructor_types: cache.protected_constructor_types,
            private_constructor_types: cache.private_constructor_types,
            all_arenas: None,
            resolved_modules: None,
            lib_contexts: Vec::new(),
            flow_graph,
            async_depth: 0,
        }
    }

    /// Create a new CheckerContext with explicit compiler options and a persistent cache.
    pub fn with_cache_and_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
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
            report_unresolved_imports: true,
            symbol_types: cache.symbol_types,
            var_decl_types: FxHashMap::default(),
            node_types: cache.node_types,
            type_parameter_names: cache.type_parameter_names,
            relation_cache: RefCell::new(cache.relation_cache),
            type_environment: RefCell::new(None),
            application_eval_cache: FxHashMap::default(),
            application_eval_set: FxHashSet::default(),
            mapped_eval_cache: FxHashMap::default(),
            mapped_eval_set: FxHashSet::default(),
            symbol_dependencies: cache.symbol_dependencies,
            symbol_dependency_stack: Vec::new(),
            diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: HashSet::new(),
            class_instance_resolution_set: HashSet::new(),
            node_resolution_stack: Vec::new(),
            node_resolution_set: HashSet::new(),
            type_parameter_scope: HashMap::new(),
            contextual_type: None,
            instantiation_depth: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
            call_depth: RefCell::new(0),
            return_type_stack: Vec::new(),
            this_type_stack: Vec::new(),
            enclosing_class: None,
            type_env: RefCell::new(TypeEnvironment::new()),
            abstract_constructor_types: cache.abstract_constructor_types,
            protected_constructor_types: cache.protected_constructor_types,
            private_constructor_types: cache.private_constructor_types,
            all_arenas: None,
            resolved_modules: None,
            lib_contexts: Vec::new(),
            flow_graph,
            async_depth: 0,
        }
    }

    /// Set lib contexts for global type resolution.
    pub fn set_lib_contexts(&mut self, lib_contexts: Vec<LibContext>) {
        self.lib_contexts = lib_contexts;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Vec<Arc<NodeArena>>) {
        self.all_arenas = Some(arenas);
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

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        TypeCache {
            symbol_types: self.symbol_types,
            node_types: self.node_types,
            type_parameter_names: self.type_parameter_names,
            relation_cache: self.relation_cache.into_inner(),
            symbol_dependencies: self.symbol_dependencies,
            abstract_constructor_types: self.abstract_constructor_types,
            protected_constructor_types: self.protected_constructor_types,
            private_constructor_types: self.private_constructor_types,
        }
    }

    /// Add an error diagnostic.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        self.diagnostics.push(Diagnostic::error(
            self.file_name.clone(),
            start,
            length,
            message,
            code,
        ));
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
}
