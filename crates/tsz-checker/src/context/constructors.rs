//! `CheckerContext` constructor methods.
//!
//! All constructors delegate to `base()` for the ~150 shared field initializations,
//! then override only the fields that differ. This eliminates massive code duplication
//! and ensures new fields automatically get default values in all constructors.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use crate::context::{CheckerContext, TypeCache};
use crate::control_flow::FlowGraph;
use tsz_binder::BinderState;
use tsz_common::checker_options::CheckerOptions;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::def::DefinitionStore;
use tsz_solver::{QueryDatabase, TypeEnvironment};

impl<'a> CheckerContext<'a> {
    fn normalize_options(
        types: &dyn QueryDatabase,
        compiler_options: CheckerOptions,
        configure_no_unchecked_indexed_access: bool,
    ) -> CheckerOptions {
        // Note: apply_strict_defaults() is intentionally NOT called here.
        // The driver/config layer already handles strict expansion with proper
        // individual overrides (e.g., strict: true + strictPropertyInitialization: false).
        // Calling apply_strict_defaults() here would clobber those overrides.
        if configure_no_unchecked_indexed_access {
            types.set_no_unchecked_indexed_access(compiler_options.no_unchecked_indexed_access);
        }
        compiler_options
    }

    /// Create a fully-initialized `CheckerContext` with all fields set to defaults.
    ///
    /// This is the single source of truth for field initialization. All public
    /// constructors call this and then override the few fields that differ.
    fn base(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        capabilities: crate::query_boundaries::capabilities::EnvironmentCapabilities,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
            report_unresolved_imports: false,
            file_is_esm: None,
            file_is_esm_map: None,
            spelling_suggestions_emitted: 0,
            no_implicit_override: false,
            types_extending_array: FxHashSet::default(),
            symbol_types: crate::context::SymbolTypeCache::with_capacity(binder.symbols.len()),
            symbol_instance_types: crate::context::SymbolTypeCache::with_capacity(
                binder.symbols.len(),
            ),
            enum_namespace_types: FxHashMap::default(),
            var_decl_types: FxHashMap::default(),
            lib_type_resolution_cache: FxHashMap::default(),
            lib_delegation_cache: FxHashMap::default(),
            shared_lib_type_cache: None,
            skip_lib_type_resolution: false,
            lib_heritage_in_progress: FxHashSet::default(),
            node_types: crate::context::NodeTypeCache::with_capacity(arena.nodes.len()),
            request_node_types: FxHashMap::default(),
            request_cache_counters: crate::context::RequestCacheCounters::default(),
            type_environment: RefCell::new(TypeEnvironment::new()),
            application_eval_set: FxHashSet::default(),
            mapped_eval_set: FxHashSet::default(),
            type_resolution_visiting: FxHashSet::default(),
            pruning_union_members: false,
            jsdoc_typedef_resolving: RefCell::new(FxHashSet::default()),
            flow_analysis_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                128,
                Default::default(),
            )),
            narrowable_identifier_cache: RefCell::new(
                crate::context::NarrowableIdentifierCache::with_capacity(arena.nodes.len()),
            ),
            flow_switch_reference_cache: RefCell::new(FxHashMap::default()),
            flow_numeric_atom_cache: RefCell::new(FxHashMap::default()),
            flow_worklist: RefCell::new(VecDeque::with_capacity(32)),
            flow_in_worklist: RefCell::new(FxHashSet::default()),
            flow_visited: RefCell::new(FxHashSet::default()),
            flow_results: RefCell::new(FxHashMap::with_capacity_and_hasher(64, Default::default())),
            flow_reference_match_cache: RefCell::new(FxHashMap::default()),
            symbol_last_assignment_pos: RefCell::new(FxHashMap::default()),
            symbol_flow_confirmed: RefCell::new(FxHashMap::default()),
            narrowing_cache: tsz_solver::NarrowingCache::new(),
            call_type_predicates: FxHashMap::default(),
            daa_error_nodes: FxHashSet::default(),
            deferred_ts2454_errors: Vec::new(),
            flow_narrowed_nodes: FxHashSet::with_capacity_and_hasher(256, Default::default()),
            refs_resolved: FxHashSet::default(),
            application_symbols_resolved: FxHashSet::default(),
            application_symbols_resolution_set: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
            js_export_surface_cache: FxHashMap::default(),
            js_export_surface_resolution_set: FxHashSet::default(),
            module_specifiers: FxHashMap::default(),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            class_constructor_type_cache: FxHashMap::default(),
            class_chain_summary_cache: RefCell::new(FxHashMap::default()),
            env_eval_cache: RefCell::new(FxHashMap::default()),
            class_symbol_to_decl_cache: RefCell::new(FxHashMap::default()),
            heritage_symbol_cache: RefCell::new(FxHashMap::default()),
            base_constructor_expr_cache: RefCell::new(FxHashMap::default()),
            base_instance_expr_cache: RefCell::new(FxHashMap::default()),
            class_decl_miss_cache: RefCell::new(FxHashSet::default()),
            jsx_intrinsic_props_cache: FxHashMap::default(),
            jsx_import_source_checked: false,
            deferred_jsx_import_source_error: None,
            symbol_dependencies: FxHashMap::default(),
            symbol_dependency_stack: Vec::new(),
            referenced_symbols: std::cell::RefCell::new(FxHashSet::default()),
            written_symbols: std::cell::RefCell::new(FxHashSet::default()),
            referenced_as_property: std::cell::RefCell::new(FxHashSet::default()),
            destructured_bindings: FxHashMap::default(),
            next_binding_group_id: 0,
            destructured_binding_sources: FxHashMap::default(),
            has_parse_errors: false,
            has_syntax_parse_errors: false,
            syntax_parse_error_positions: Vec::new(),
            has_real_syntax_errors: false,
            real_syntax_error_positions: Vec::new(),
            all_parse_error_positions: Vec::new(),
            nullable_type_parse_error_positions: Vec::new(),
            diagnostics: Vec::new(),
            emitted_diagnostics: FxHashSet::default(),
            callback_return_type_errors: Vec::new(),
            modules_with_ts2307_emitted: FxHashSet::default(),
            deferred_truthiness_diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: FxHashSet::default(),
            circular_type_aliases: FxHashSet::default(),
            import_conflict_names: FxHashSet::default(),
            module_namespace_resolution_set: FxHashSet::default(),
            import_type_alias_types: FxHashMap::default(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: super::MAX_SYMBOL_RESOLUTION_DEPTH,
            class_instance_resolution_set: FxHashSet::default(),
            class_constructor_resolution_set: FxHashSet::default(),
            circular_class_symbols: FxHashSet::default(),
            pending_implicit_any_vars: FxHashMap::default(),
            pending_circular_return_sites: FxHashMap::default(),
            non_closure_circular_return_tracking_depth: 0,
            reported_implicit_any_vars: FxHashMap::default(),
            inheritance_graph: tsz_solver::classes::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            implicit_any_checked_closures: FxHashSet::default(),
            implicit_any_contextual_closures: FxHashSet::default(),
            deferred_implicit_any_closures: Vec::new(),
            speculative_implicit_any_closures: Vec::new(),
            checking_classes: FxHashSet::default(),
            checked_classes: FxHashSet::default(),
            checking_computed_property_name: None,
            type_parameter_scope: FxHashMap::default(),
            in_conditional_extends_depth: 0,
            typeof_param_scope: FxHashMap::default(),
            contextual_type: None,
            contextual_type_is_assertion: false,
            is_checking_statements: false,
            is_in_ambient_declaration_file: false,
            in_destructuring_target: false,
            skip_flow_narrowing: false,
            instantiation_depth: Cell::new(0),
            depth_exceeded: Cell::new(false),
            relation_depth_exceeded: Cell::new(false),
            eval_session: Rc::new(tsz_solver::EvaluationSession::new()),
            recursion_depth: RefCell::new(tsz_solver::recursion::DepthCounter::with_profile(
                tsz_solver::recursion::RecursionProfile::CheckerRecursion,
            )),
            heritage_merge_depth: Cell::new(0),
            call_depth: RefCell::new(tsz_solver::recursion::DepthCounter::with_profile(
                tsz_solver::recursion::RecursionProfile::CallResolution,
            )),
            circ_ref_depth: RefCell::new(tsz_solver::recursion::DepthCounter::new(30)),
            overlap_depth: RefCell::new(tsz_solver::recursion::DepthCounter::new(20)),
            resolving_jsdoc_typedefs: RefCell::new(Vec::new()),
            jsdoc_typedef_anchor_pos: std::cell::Cell::new(u32::MAX),
            return_type_stack: Vec::new(),
            yield_type_stack: Vec::new(),
            generator_next_type_stack: Vec::new(),
            generator_yield_operand_types: Vec::new(),
            generator_had_ts7057: false,
            this_type_stack: Vec::new(),
            enclosing_class: None,
            enclosing_class_chain: Vec::new(),
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_to_symbol: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            def_no_type_params: RefCell::new(FxHashSet::default()),
            def_fallback_count: Cell::new(0),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            cross_file_symbol_targets: RefCell::new(FxHashMap::default()),
            global_symbol_file_index: None,
            all_arenas: None,
            all_binders: None,
            global_file_locals_index: None,
            global_module_exports_index: None,
            global_declared_modules: None,
            global_expando_index: None,
            global_module_augmentations_index: None,
            global_augmentation_targets_index: None,
            global_module_binder_index: None,
            global_arena_index: None,
            resolved_module_paths: None,
            resolved_module_request_paths: None,
            current_file_idx: 0,
            resolved_modules: None,
            module_augmentation_value_decls: FxHashMap::default(),
            module_augmentation_application_set: RefCell::new(FxHashSet::default()),
            is_external_module_by_file: None,
            resolved_module_errors: None,
            resolved_module_request_errors: None,
            import_resolution_stack: Vec::new(),
            type_only_nodes: FxHashSet::default(),
            lib_contexts: Arc::new(Vec::new()),
            lib_binders_cached: Arc::new(Vec::new()),
            actual_lib_file_count: 0,
            typescript_dom_replacement_loaded: false,
            typescript_dom_replacement_has_window: false,
            typescript_dom_replacement_has_self: false,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            in_const_assertion: false,
            preserve_literal_types: false,
            generic_excess_skip: None,
            iteration_depth: 0,
            switch_depth: 0,
            function_depth: 0,
            is_unreachable: false,
            has_reported_unreachable: false,
            label_stack: Vec::new(),
            had_outer_loop: false,
            suppress_definite_assignment_errors: false,
            js_body_uses_arguments: false,
            emitted_ts2454_errors: FxHashSet::default(),
            type_resolution_fuel: Cell::new(crate::state::MAX_TYPE_RESOLUTION_OPS),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
        }
    }

    /// Create a new `CheckerContext`.
    ///
    /// Creates a pre-populated `DefinitionStore` from the binder's
    /// `semantic_defs` at construction time, using the solver-owned
    /// `DefinitionStore::from_semantic_defs` factory. This moves
    /// identity creation entirely out of checker code into the solver.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
    ) -> Self {
        let compiler_options = Self::normalize_options(types, compiler_options, true);
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        // Create pre-populated DefinitionStore from binder's semantic_defs
        // using the solver-owned factory. This is the canonical identity
        // creation path — no checker-side conversion needed.
        ctx.definition_store = Arc::new(DefinitionStore::from_semantic_defs(
            &binder.semantic_defs,
            |s| types.intern_string(s),
        ));
        ctx.warm_local_caches_from_shared_store();
        ctx
    }

    /// Create a new `CheckerContext` with a shared `DefinitionStore`.
    ///
    /// This allows multiple contexts (e.g., main file + lib files) to share the same
    /// `DefId` namespace, preventing `DefId` collisions where different symbols would
    /// otherwise get the same `DefId` from independent stores.
    ///
    /// # Arguments
    /// * `definition_store` - Shared `DefinitionStore` (wrapped in Arc for thread-safety)
    /// * Other args same as `new()`
    pub fn new_with_shared_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        definition_store: Arc<DefinitionStore>,
    ) -> Self {
        let compiler_options = Self::normalize_options(types, compiler_options, true);
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        ctx.definition_store = definition_store;
        // Eagerly warm local caches from the shared store so that
        // cross-file symbol resolution and other early-access paths
        // hit O(1) local lookups instead of the fallback path.
        ctx.warm_local_caches_from_shared_store();
        ctx
    }

    /// Create a new `CheckerContext` with explicit compiler options.
    ///
    /// Creates a pre-populated `DefinitionStore` from the binder's
    /// `semantic_defs` using the solver-owned factory.
    pub fn with_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: &CheckerOptions,
    ) -> Self {
        let compiler_options = Self::normalize_options(types, compiler_options.clone(), true);
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        ctx.definition_store = Arc::new(DefinitionStore::from_semantic_defs(
            &binder.semantic_defs,
            |s| types.intern_string(s),
        ));
        ctx.warm_local_caches_from_shared_store();
        ctx
    }

    /// Apply `TypeCache` fields to a context, overriding the defaults.
    ///
    /// This centralizes the cache-restoration logic shared by `with_cache`
    /// and `with_cache_and_options`.
    fn apply_cache(&mut self, cache: TypeCache) {
        self.symbol_types = cache.symbol_types;
        self.symbol_instance_types = cache.symbol_instance_types;
        // node_types is per-arena (keyed by raw node index u32), so it must NOT
        // be carried across files — indices from file A collide with file B.
        // We keep the fresh per-arena allocation from base().
        self.flow_analysis_cache = RefCell::new(cache.flow_analysis_cache);
        // Reset flow worklist/visited buffers since they had pre-allocated capacity
        // in base() but cache path historically used empty defaults.
        self.flow_worklist = RefCell::new(VecDeque::new());
        self.namespace_module_names = cache.namespace_module_names;
        self.class_instance_type_to_decl = cache.class_instance_type_to_decl;
        self.class_instance_type_cache = cache.class_instance_type_cache;
        self.class_constructor_type_cache = cache.class_constructor_type_cache;
        self.symbol_dependencies = cache.symbol_dependencies;
        self.def_to_symbol = RefCell::new(cache.def_to_symbol);
    }

    /// Create a new `CheckerContext` with a persistent cache.
    ///
    /// NOTE: `cache.node_types` is intentionally dropped here. Node indices are
    /// per-arena (each file has its own `NodeArena` starting from 0), so carrying
    /// node type entries across files would cause index collisions — e.g., node 12
    /// in `react.d.ts` would shadow node 12 in the user's file, corrupting type
    /// resolution for heritage clauses and property access expressions.
    pub fn with_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        ctx.definition_store = Arc::new(DefinitionStore::from_semantic_defs(
            &binder.semantic_defs,
            |s| types.intern_string(s),
        ));
        ctx.apply_cache(cache);
        ctx.warm_local_caches_from_shared_store();
        ctx
    }

    /// Create a new `CheckerContext` with explicit compiler options and a persistent cache.
    ///
    /// Creates a pre-populated `DefinitionStore` from the binder's
    /// `semantic_defs` using the solver-owned factory.
    pub fn with_cache_and_options(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: &CheckerOptions,
    ) -> Self {
        let compiler_options = compiler_options.clone().apply_strict_defaults();
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        ctx.definition_store = Arc::new(DefinitionStore::from_semantic_defs(
            &binder.semantic_defs,
            |s| types.intern_string(s),
        ));
        ctx.apply_cache(cache);
        ctx.warm_local_caches_from_shared_store();
        ctx
    }

    /// Create a new `CheckerContext` with a persistent cache and a shared `DefinitionStore`.
    ///
    /// Combines cache restoration with shared definition store, which is needed
    /// by the LSP to reuse type checking results across edits while keeping all
    /// files' definitions in a single `DefId` namespace.
    pub fn with_cache_and_shared_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: CheckerOptions,
        definition_store: Arc<DefinitionStore>,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                false,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );
        ctx.definition_store = definition_store;
        ctx.warm_local_caches_from_shared_store();
        ctx.apply_cache(cache);
        ctx
    }

    /// Create a child `CheckerContext` for temporary cross-file checks.
    ///
    /// Important: only caches keyed by globally stable ids (e.g. `TypeId`, `RelationCacheKey`)
    /// are copied from the parent. Arena/binder-local ids (`SymbolId`, `NodeIndex`, `FlowNodeId`)
    /// must be reset to avoid cross-arena cache poisoning.
    pub fn with_parent_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &Self,
    ) -> Self {
        let compiler_options = compiler_options.apply_strict_defaults();
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                parent.capabilities.has_lib,
            );
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
        );

        // Propagate parent state that is safe across arenas.
        ctx.no_implicit_override = parent.no_implicit_override;

        // Share symbol caches: after merge, all binders use global SymbolIds,
        // so SymbolId(N) means the same entity regardless of which arena/binder
        // the child checker operates on. Sharing avoids redundant re-resolution
        // of lib types (Array, Promise, etc.) in each child context.
        ctx.symbol_types = parent.symbol_types.clone();
        ctx.symbol_instance_types = parent.symbol_instance_types.clone();
        ctx.enum_namespace_types = parent.enum_namespace_types.clone();

        ctx.lib_delegation_cache = parent.lib_delegation_cache.clone();
        ctx.skip_lib_type_resolution = parent.skip_lib_type_resolution;

        // CRITICAL: Propagate in-progress set from parent to prevent re-entrant
        // heritage merging in child contexts (cross-arena delegation). Without this,
        // child CheckerStates don't see that the parent is already resolving a type,
        // causing unbounded mutual recursion through resolve_lib_type_by_name ↔
        // merge_lib_interface_heritage ↔ build_type_environment chains.
        ctx.lib_heritage_in_progress = parent.lib_heritage_in_progress.clone();

        // Propagate depth from parent to prevent infinite recursion across arena boundaries.
        ctx.symbol_resolution_depth = Cell::new(parent.symbol_resolution_depth.get());

        // Share evaluation session with parent so depth/fuel counters survive
        // cross-arena delegation (replaces thread-local guards).
        ctx.eval_session = Rc::clone(&parent.eval_session);

        ctx.implicit_any_checked_closures = parent.implicit_any_checked_closures.clone();
        ctx.implicit_any_contextual_closures = parent.implicit_any_contextual_closures.clone();

        // Propagate depth from parent to prevent infinite recursion across arena boundaries.
        ctx.recursion_depth =
            RefCell::new(tsz_solver::recursion::DepthCounter::with_initial_depth(
                tsz_solver::recursion::RecursionProfile::CheckerRecursion.max_depth(),
                parent.recursion_depth.borrow().depth(),
            ));
        ctx.heritage_merge_depth = Cell::new(parent.heritage_merge_depth.get());

        // Share DefinitionStore with parent so DefIds are globally unique
        // across parent/child checkers. This prevents DefId collisions where
        // the child's DefId(1) means a different thing than the parent's DefId(1).
        ctx.definition_store = Arc::clone(&parent.definition_store);

        ctx
    }
}
