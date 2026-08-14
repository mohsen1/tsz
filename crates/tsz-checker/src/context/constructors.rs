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

use crate::context::{CheckerContext, DiagnosticIndices, PendingCircularReturnSites, TypeCache};
use crate::control_flow::FlowGraph;
use crate::query_boundaries::common::{QueryDatabase, TypeEnvironment};
use tsz_binder::BinderState;
use tsz_common::checker_options::CheckerOptions;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::def::DefinitionStore;

/// Compiler-option finalization policy for one construction path.
///
/// Historically every constructor hand-rolled this step and drifted in two
/// independent dimensions (whether `apply_strict_defaults` ran, and whether
/// the index flags were pushed into the `QueryDatabase`). The policy makes
/// both explicit per constructor; the resulting matrix is pinned by the
/// `checker_constructor_matrix_tests` integration test so future drift fails
/// loudly instead of diverging silently.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OptionsPolicy {
    /// Expand `strict` into the strict-family sub-flags via
    /// [`CheckerOptions::apply_strict_defaults`].
    ///
    /// Preserved legacy behavior: when the caller already resolved options at
    /// the config layer (e.g. `strict: true` plus an explicit
    /// `strictPropertyInitialization: false` override), expanding again here
    /// clobbers the per-flag opt-out. Paths that receive pre-resolved options
    /// must use [`OptionsPolicy::PRE_RESOLVED`].
    expand_strict: bool,
    /// Push `no_unchecked_indexed_access` / `exact_optional_property_types`
    /// into the `QueryDatabase` (the historical `normalize_options` side
    /// effect). Cache/parent constructors historically skipped this and rely
    /// on the driver or the owning context having configured the database.
    push_index_flags_into_types: bool,
}

impl OptionsPolicy {
    /// The driver/config layer fully resolved the options (strict family
    /// expanded with individual overrides honored): do not re-expand here;
    /// push the index flags into the `QueryDatabase`.
    pub(crate) const PRE_RESOLVED: Self = Self {
        expand_strict: false,
        push_index_flags_into_types: true,
    };
    /// Expand the strict family here; leave the `QueryDatabase` untouched.
    /// Historical behavior of the cache and parent-cache constructors.
    pub(crate) const EXPAND_STRICT_LOCALLY: Self = Self {
        expand_strict: true,
        push_index_flags_into_types: false,
    };
    /// Expand the strict family here and push the index flags into the
    /// `QueryDatabase`. Historical behavior of
    /// `CheckerState::with_options_and_shared_def_store` (which used to call
    /// `apply_strict_defaults` at the state layer before delegating).
    pub(crate) const EXPAND_STRICT_AND_PUSH: Self = Self {
        expand_strict: true,
        push_index_flags_into_types: true,
    };
}

/// How the context's `DefinitionStore` is installed.
pub(crate) enum DefStorePlan {
    /// Build a per-file store from the binder's `semantic_defs` via the
    /// solver-owned factory, then warm the local symbol/def caches from it.
    PerFile,
    /// Install a shared store (project-wide `DefId` namespace), then warm
    /// the local symbol/def caches from it.
    Shared(Arc<DefinitionStore>),
    /// Leave the empty default store and skip warm-up. The caller MUST
    /// install a populated store before use (`ProgramContext::apply_to`, or
    /// the parent-propagation block in `with_parent_cache`).
    Deferred,
}

/// When a persistent [`TypeCache`] is restored relative to
/// `warm_local_caches_from_shared_store`.
///
/// The order is observable: `apply_cache` replaces `def_to_symbol`
/// wholesale, while warm-up inserts into it. Both historical orders are
/// preserved explicitly per constructor.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CacheRestoreOrder {
    /// Restore the cache before warming local caches (historical
    /// `with_cache` / `with_cache_and_options` order: warmed mappings land
    /// on top of the restored cache).
    BeforeWarm,
    /// Restore the cache after warming local caches (historical
    /// `with_cache_and_shared_def_store` order: the restored `def_to_symbol`
    /// replaces warmed entries).
    AfterWarm,
}

/// Per-constructor parts consumed by [`CheckerContext::from_parts`], the
/// single private build path behind every public constructor.
pub(crate) struct ContextParts {
    pub(crate) file_name: String,
    pub(crate) compiler_options: CheckerOptions,
    pub(crate) options_policy: OptionsPolicy,
    pub(crate) def_store: DefStorePlan,
    /// Persistent cache to restore, if any.
    pub(crate) cache: Option<TypeCache>,
    /// Ignored when `cache` is `None`.
    pub(crate) cache_order: CacheRestoreOrder,
    /// `EnvironmentCapabilities::from_options` `has_lib` seed; only the
    /// parent-cache path inherits this from the parent context.
    pub(crate) inherit_has_lib: bool,
    /// Capacity for the symbol type caches; `None` means
    /// `binder.symbols.len()`. Child contexts pass `Some(0)` because they
    /// replace the caches with parent snapshots immediately afterwards.
    pub(crate) symbol_cache_capacity: Option<usize>,
}

impl<'a> CheckerContext<'a> {
    /// Single build path for every public constructor: finalize options
    /// exactly once per the explicit [`OptionsPolicy`], derive capabilities,
    /// initialize all fields via [`Self::base`], then install the
    /// `DefinitionStore` and restore the persistent cache per plan.
    ///
    /// `apply_strict_defaults` has exactly one call site in this crate: here.
    pub(crate) fn from_parts(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        parts: ContextParts,
    ) -> Self {
        let ContextParts {
            file_name,
            compiler_options,
            options_policy,
            def_store,
            cache,
            cache_order,
            inherit_has_lib,
            symbol_cache_capacity,
        } = parts;

        // Note: for `PRE_RESOLVED`, `apply_strict_defaults()` is intentionally
        // NOT called. The driver/config layer already handles strict expansion
        // with proper individual overrides (e.g., strict: true +
        // strictPropertyInitialization: false). Calling it here would clobber
        // those overrides.
        let compiler_options = if options_policy.expand_strict {
            compiler_options.apply_strict_defaults()
        } else {
            compiler_options
        };
        if options_policy.push_index_flags_into_types {
            types.set_no_unchecked_indexed_access(compiler_options.no_unchecked_indexed_access);
            types.set_exact_optional_property_types(compiler_options.exact_optional_property_types);
        }
        // Wire strictNullChecks unconditionally: it gates whether optional-member
        // access/call/inference types carry `| undefined` (tsc's addOptionality is
        // strictNullChecks-gated). The interner defaults to `true`, so this must run
        // for a non-strict compilation to actually strip the synthetic `undefined`.
        types.set_strict_null_checks(compiler_options.strict_null_checks);
        let capabilities =
            crate::query_boundaries::capabilities::EnvironmentCapabilities::from_options(
                &compiler_options,
                inherit_has_lib,
            );
        let symbol_cache_capacity = symbol_cache_capacity.unwrap_or_else(|| binder.symbols.len());
        let mut ctx = Self::base(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            capabilities,
            symbol_cache_capacity,
        );

        let warm = match def_store {
            DefStorePlan::PerFile => {
                // Pre-populated `DefinitionStore` from the binder's
                // `semantic_defs` using the solver-owned factory. This is the
                // canonical identity creation path — no checker-side
                // conversion needed.
                ctx.definition_store = Arc::new(DefinitionStore::from_semantic_defs(
                    &binder.semantic_defs,
                    |s| types.intern_string(s),
                ));
                true
            }
            DefStorePlan::Shared(store) => {
                ctx.definition_store = store;
                true
            }
            DefStorePlan::Deferred => false,
        };
        match cache {
            None => {
                if warm {
                    ctx.warm_local_caches_from_shared_store();
                }
            }
            Some(cache) => match cache_order {
                CacheRestoreOrder::BeforeWarm => {
                    ctx.apply_cache(cache);
                    if warm {
                        ctx.warm_local_caches_from_shared_store();
                    }
                }
                CacheRestoreOrder::AfterWarm => {
                    if warm {
                        ctx.warm_local_caches_from_shared_store();
                    }
                    ctx.apply_cache(cache);
                }
            },
        }
        ctx
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
        symbol_cache_capacity: usize,
    ) -> Self {
        let flow_graph = Some(FlowGraph::new(&binder.flow_nodes));

        CheckerContext {
            arena,
            binder,
            types,
            file_name,
            current_directory: None,
            compiler_options,
            capabilities,
            report_unresolved_imports: false,
            allow_source_file_test_pragmas: false,
            file_is_esm: None,
            file_is_esm_map: None,
            name_resolution_diagnostics: crate::context::NameResolutionDiagnostics::default(),
            no_implicit_override: false,
            types_extending_array: FxHashSet::default(),
            recovery_sites: RefCell::new(crate::recovery::RecoverySites::default()),
            symbol_types: crate::context::SymbolTypeCache::with_capacity(symbol_cache_capacity),
            symbol_instance_types: crate::context::SymbolTypeCache::with_capacity(
                symbol_cache_capacity,
            ),
            enum_namespace_types: crate::context::CowCache::default(),
            var_decl_types: FxHashMap::default(),
            lib_type_resolution_caches: crate::context::LibTypeResolutionCaches::default(),
            lib_delegation_cache: crate::context::CrossFileDelegationCache::default(),
            namespace_member_resolution_cache: RefCell::new(crate::context::CowCache::default()),
            export_equals_named_cache: RefCell::new(crate::context::CowCache::default()),
            nested_namespace_candidates_cache: RefCell::new(crate::context::CowCache::default()),
            symbol_name_candidates_cache: RefCell::new(FxHashMap::default()),
            member_access_info_cache: RefCell::new(FxHashMap::default()),
            enclosing_class_declares_member_cache: RefCell::new(FxHashMap::default()),
            accessor_levels_cache: RefCell::new(FxHashMap::default()),
            jsdoc_global_typedef_lookup_cache: crate::context::JSDocGlobalTypedefLookupCache {
                miss_cache: RefCell::new(FxHashSet::default()),
                in_progress: RefCell::new(FxHashSet::default()),
                typedef_presence_by_file: Arc::new(dashmap::DashMap::new()),
                tag_presence_by_file: Arc::new(dashmap::DashMap::new()),
            },
            nested_namespace_candidates_cache_complete: Cell::new(false),
            lowering_entity_name_resolution_cache: RefCell::new(crate::context::CowCache::default()),
            namespace_exports_cache: RefCell::new(FxHashMap::default()),
            reexport_resolution_cache: RefCell::new(FxHashMap::default()),
            shared_lib_type_cache: None,
            shared_constraint_proofs: None,
            cross_file_type_params_cache: None,
            skip_lib_type_resolution: false,
            lib_heritage_in_progress: crate::context::CowCache::default(),
            node_types: crate::context::NodeTypeCache::with_capacity(arena.nodes.len()),
            request_node_types: crate::context::CowCache::default(),
            object_literal_tracking: crate::context::ObjectLiteralTracking::default(),
            request_cache_counters: crate::context::RequestCacheCounters::default(),
            type_environment: RefCell::new(TypeEnvironment::new()),
            deferred_flow_env_writes: RefCell::new(Vec::new()),
            deferred_eval_env_writes: RefCell::new(Vec::new()),
            application_eval_set: FxHashSet::default(),
            mapped_eval_set: FxHashSet::default(),
            type_resolution_visiting: FxHashSet::default(),
            pruning_union_members: false,
            jsdoc_typedef_resolving: RefCell::new(crate::context::CowCache::default()),
            jsdoc_generic_typedef_resolving: RefCell::new(crate::context::CowCache::default()),
            flow_shared: crate::context::FlowSharedCaches::new(),
            narrowable_identifier_cache: RefCell::new(
                crate::context::NarrowableIdentifierCache::with_capacity(arena.nodes.len()),
            ),
            symbol_flow_confirmed: RefCell::new(crate::context::CowCache::default()),
            daa_error_nodes: crate::context::CowCache::default(),
            optional_chain_marker_only_nodes: FxHashSet::default(),
            noinfer_generic_return_bodies: FxHashSet::default(),
            deferred_ts2454_errors: Vec::new(),
            flow_narrowed_nodes: crate::context::CowCache::new(
                FxHashSet::with_capacity_and_hasher(256, Default::default()),
            ),
            refs_resolved: FxHashSet::default(),
            application_symbols_resolved: FxHashSet::default(),
            application_symbols_resolution_set: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
            js_export_surface_cache: FxHashMap::default(),
            js_export_surface_resolution_set: FxHashMap::default(),
            expando_property_resolution_set: crate::context::CowCache::default(),
            module_specifiers: Arc::new(FxHashMap::default()),
            module_path_specifiers: Arc::new(FxHashMap::default()),
            module_specifiers_prebuilt: false,
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: RefCell::new(FxHashMap::default()),
            class_constructor_type_cache: RefCell::new(FxHashMap::default()),
            class_chain_summary_cache: RefCell::new(FxHashMap::default()),
            env_eval_cache: RefCell::new(crate::context::env_eval_cache::EnvEvalCache::default()),
            lazy_def_ids_cache: RefCell::new(FxHashMap::default()),
            type_queries_cache: RefCell::new(FxHashMap::default()),
            type_position_resolution_cache: RefCell::new(FxHashMap::default()),
            package_json_cache: RefCell::new(FxHashMap::default()),
            class_symbol_to_decl_cache: RefCell::new(FxHashMap::default()),
            heritage_symbol_cache: RefCell::new(FxHashMap::default()),
            base_constructor_expr_cache: RefCell::new(FxHashMap::default()),
            base_instance_expr_cache: RefCell::new(FxHashMap::default()),
            class_decl_miss_cache: RefCell::new(FxHashSet::default()),
            jsx_intrinsic_props_cache: FxHashMap::default(),
            jsx_namespace_symbol_cache: None,
            jsx_intrinsic_elements_symbol_cache: None,
            jsx_intrinsic_elements_type_cache: None,
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
            has_structural_parse_errors: false,
            real_syntax_error_positions: Vec::new(),
            all_parse_error_positions: Vec::new(),
            nullable_type_parse_error_positions: Vec::new(),
            parameter_grammar_suppress_spans: Vec::new(),
            diagnostics: Vec::new(),
            diagnostics_discarded: false,
            diagnostic_indices: DiagnosticIndices::default(),
            no_overload_call_nodes: crate::context::CowCache::default(),
            callback_return_type_errors: Vec::new(),
            modules_with_ts2307_emitted: crate::context::CowCache::default(),
            deferred_truthiness_diagnostics: Vec::new(),
            deferred_excess_property_implicit_any_diagnostics: Vec::new(),
            symbol_resolution_stack: Vec::new(),
            symbol_resolution_set: FxHashSet::default(),
            circular_type_aliases: FxHashSet::default(),
            import_conflict_names: FxHashSet::default(),
            module_namespace_resolution_set: FxHashSet::default(),
            import_type_alias_types: FxHashMap::default(),
            merged_value_types: FxHashMap::default(),
            symbol_resolution_depth: Cell::new(0),
            max_symbol_resolution_depth: super::MAX_SYMBOL_RESOLUTION_DEPTH,
            class_instance_resolution_set: FxHashSet::default(),
            class_constructor_resolution_set: FxHashSet::default(),
            window_partial_ctor_types: FxHashMap::default(),
            jsdoc_enum_resolution_set: FxHashSet::default(),
            circular_class_symbols: FxHashSet::default(),
            pending_implicit_any_vars: crate::context::CowCache::default(),
            pending_circular_return_sites: PendingCircularReturnSites::default(),
            non_closure_circular_return_tracking_depth: 0,
            inferred_return_type_memo: FxHashMap::default(),
            callback_mismatch_memo: FxHashMap::default(),
            reported_implicit_any_vars: crate::context::CowCache::default(),
            inheritance_graph: tsz_solver::classes::inheritance::InheritanceGraph::new(),
            node_resolution_stack: Vec::new(),
            implicit_any_checked_closures: crate::context::CowCache::default(),
            implicit_any_contextual_closures: crate::context::CowCache::default(),
            deferred_implicit_any_closures: Vec::new(),
            speculative_implicit_any_closures: Vec::new(),
            closures_with_contextual_this_type: FxHashSet::default(),
            checking_classes: FxHashSet::default(),
            checked_classes: FxHashSet::default(),
            checking_computed_property_name: None,
            type_parameter_scope: FxHashMap::default(),
            type_reference_validation_caches:
                crate::context::TypeReferenceValidationCaches::default(),
            in_conditional_extends_depth: 0,
            typeof_param_scope: FxHashMap::default(),
            type_param_constraint_excluded_params: FxHashSet::default(),
            contextual_type: None,
            contextual_type_is_assertion: false,
            is_checking_statements: false,
            is_in_ambient_declaration_file: false,
            in_destructuring_target: false,
            preserve_destructuring_initializer_overload_diagnostics: false,
            skip_flow_narrowing: false,
            instantiation_depth: Cell::new(0),
            depth_exceeded: Cell::new(false),
            relation_overflow: Cell::new(crate::context::RelationOverflowFlags::default()),
            skip_callable_type_param_suppression: Cell::new(false),
            eval_session: Rc::new(tsz_solver::evaluation::session::EvaluationSession::new()),
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
            function_owned_this_stack: Vec::new(),
            enclosing_class: None,
            enclosing_class_chain: Vec::new(),
            type_env: RefCell::new(TypeEnvironment::new()),
            definition_store: Arc::new(DefinitionStore::new()),
            share_owner_symbol_type_results: false,
            symbol_to_def: RefCell::new(FxHashMap::default()),
            def_to_symbol: RefCell::new(FxHashMap::default()),
            def_type_params: RefCell::new(FxHashMap::default()),
            def_no_type_params: RefCell::new(FxHashSet::default()),
            def_fallback_count: Cell::new(0),
            local_caches_warmed: Cell::new(false),
            abstract_constructor_types: FxHashSet::default(),
            protected_constructor_types: FxHashSet::default(),
            private_constructor_types: FxHashSet::default(),
            cross_file_symbol_targets: RefCell::new(super::SymbolFileTargetsOverlay::default()),
            global_symbol_file_index: None,
            all_arenas: None,
            all_binders: None,
            global_file_locals_index: None,
            global_module_exports_index: None,
            global_declared_modules: None,
            global_expando_index: None,
            global_module_augmentations_index: None,
            global_scope_conflict_candidates: std::cell::OnceCell::new(),
            effective_jsx_mode_cache: std::cell::Cell::new(None),
            global_augmentation_targets_index: None,
            global_module_binder_index: None,
            global_arena_index: None,
            global_file_name_index: None,
            program_reexports: None,
            program_wildcard_reexports: None,
            program_module_exports: None,
            program_cross_file_node_symbols: None,
            program_alias_partners: None,
            resolved_module_paths: None,
            resolved_module_request_paths: None,
            resolved_module_ts_extension_flags: None,
            current_file_idx: 0,
            type_position_deprecated_import_assert_files: FxHashMap::default(),
            inference_placeholder_state: Cell::new(0),
            resolved_modules: None,
            module_augmentation_value_decls: FxHashMap::default(),
            module_augmentation_application_set: RefCell::new(FxHashSet::default()),
            is_external_module_by_file: None,
            resolved_module_errors: None,
            untyped_module_paths: None,
            resolved_module_request_errors: None,
            import_resolution_stack: Vec::new(),
            type_only_nodes: FxHashSet::default(),
            lib_contexts: Arc::new(Vec::new()),
            lib_binders_cached: Arc::new(Vec::new()),
            lib_file_local_names: None,
            actual_lib_file_count: 0,
            typescript_dom_replacement_loaded: false,
            typescript_dom_replacement_has_window: false,
            typescript_dom_replacement_has_self: false,
            flow_graph,
            async_depth: 0,
            inside_closure_depth: 0,
            in_const_assertion: false,
            in_satisfies_operand: false,
            preserve_literal_types: false,
            preserve_logical_operand_literals: false,
            use_declared_type_for_identifier: false,
            skip_array_contextual_supertype_collapse: false,
            generic_excess_skip: None,
            iteration_depth: 0,
            switch_depth: 0,
            function_depth: 0,
            class_member_body_depth: 0,
            is_unreachable: false,
            has_reported_unreachable: false,
            suppress_unreachable_reporting: false,
            label_stack: Vec::new(),
            had_outer_loop: false,
            suppress_definite_assignment_errors: false,
            js_body_uses_arguments: false,
            emitted_ts2454_errors: crate::context::CowCache::default(),
            emitted_ts2411_for_iface_prop: FxHashSet::default(),
            type_resolution_fuel: Cell::new(crate::state::MAX_TYPE_RESOLUTION_OPS),
            typeof_resolution_stack: RefCell::new(FxHashSet::default()),
            omitted_default_constraint_stack: RefCell::new(FxHashSet::default()),
            type_param_node_cache: FxHashMap::default(),
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
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::PRE_RESOLVED,
                def_store: DefStorePlan::PerFile,
                cache: None,
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
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
        // Local caches are eagerly warmed from the shared store so that
        // cross-file symbol resolution and other early-access paths
        // hit O(1) local lookups instead of the fallback path.
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::PRE_RESOLVED,
                def_store: DefStorePlan::Shared(definition_store),
                cache: None,
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
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
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options: compiler_options.clone(),
                options_policy: OptionsPolicy::PRE_RESOLVED,
                def_store: DefStorePlan::PerFile,
                cache: None,
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
    }

    /// Same as [`with_options`], but skips building the per-file
    /// `DefinitionStore` and the local-cache warm-up.
    ///
    /// **Invariant**: the caller MUST install a populated store before use,
    /// typically via `ProgramContext::apply_to` (which assigns
    /// `ctx.definition_store` from a project-wide shared store and then
    /// runs `warm_local_caches_from_shared_store`). Using the returned
    /// context without that follow-up yields an empty store and mysterious
    /// type resolution failures.
    ///
    /// This exists because the CLI's parallel checker path always calls
    /// `apply_to` immediately after construction, which overwrites the
    /// per-file store with the shared one. Building the per-file store up
    /// front (`from_semantic_defs` + `warm_local_caches_from_shared_store`
    /// twice) showed up in profiles as ~8% of total CPU on multi-file
    /// projects, all of it thrown away.
    pub fn with_options_deferred_def_store(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: &CheckerOptions,
    ) -> Self {
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options: compiler_options.clone(),
                options_policy: OptionsPolicy::PRE_RESOLVED,
                def_store: DefStorePlan::Deferred,
                cache: None,
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
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
        self.flow_shared.flow_analysis_cache =
            RefCell::new(crate::context::CowCache::new(cache.flow_analysis_cache));
        // Reset flow worklist/visited buffers since they had pre-allocated capacity
        // in base() but cache path historically used empty defaults.
        self.flow_shared.flow_worklist = RefCell::new(VecDeque::new());
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
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::EXPAND_STRICT_LOCALLY,
                def_store: DefStorePlan::PerFile,
                cache: Some(cache),
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
    }

    /// Like [`Self::with_cache`], but for callers whose `compiler_options` is
    /// already fully resolved (strict family expanded with individual
    /// overrides honored) instead of a raw `strict` umbrella.
    ///
    /// `with_cache`'s `EXPAND_STRICT_LOCALLY` policy re-runs
    /// `apply_strict_defaults()`, which re-expands from `options.strict` and
    /// silently clobbers an explicit per-member override back to the
    /// umbrella's value when `strict` itself was never set to `false` (e.g.
    /// `--strictNullChecks false` alone, with no `--strict` flag, leaves
    /// `options.strict` at its default `true`). The CLI driver's per-file
    /// cached path hits exactly this: its `compiler_options` already went
    /// through `strict_family::apply_strict_family`, so re-expanding here
    /// discards that resolution — and because `types` is one `QueryDatabase`
    /// shared across every file in the compilation, a cached lib file
    /// checked through this path clobbers `strict_null_checks` for every
    /// file checked afterwards, not just itself.
    pub fn with_cache_pre_resolved(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        cache: TypeCache,
        compiler_options: CheckerOptions,
    ) -> Self {
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::PRE_RESOLVED,
                def_store: DefStorePlan::PerFile,
                cache: Some(cache),
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
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
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options: compiler_options.clone(),
                options_policy: OptionsPolicy::EXPAND_STRICT_LOCALLY,
                def_store: DefStorePlan::PerFile,
                cache: Some(cache),
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
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
        Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::EXPAND_STRICT_LOCALLY,
                def_store: DefStorePlan::Shared(definition_store),
                cache: Some(cache),
                cache_order: CacheRestoreOrder::AfterWarm,
                inherit_has_lib: false,
                symbol_cache_capacity: None,
            },
        )
    }

    /// Create a child `CheckerContext` for temporary cross-file checks.
    ///
    /// Important: only caches keyed by globally stable ids (e.g. `TypeId`, `RelationCacheKey`)
    /// are copied from the parent. Arena/binder-local ids (`SymbolId`, `NodeIndex`, `FlowNodeId`)
    /// must be reset to avoid cross-arena cache poisoning.
    ///
    /// `compiler_options` is always inherited from a parent context (the sole
    /// production caller, `CheckerState::delegate_for_arena`, passes
    /// `parent.ctx.compiler_options.clone()`), so it is already fully
    /// resolved — never a raw `strict` umbrella needing local expansion.
    /// `PRE_RESOLVED` avoids re-running `apply_strict_defaults()`, which
    /// would re-expand from `options.strict` and clobber an explicit
    /// per-member override the parent already resolved (e.g.
    /// `strictNullChecks: false` with no `--strict` flag leaves
    /// `options.strict` at its default `true`) — and because `types` is a
    /// `QueryDatabase` shared with the parent (and every other file in the
    /// same compilation), that clobbered flag leaks beyond this one child.
    pub fn with_parent_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &Self,
    ) -> Self {
        let mut ctx = Self::from_parts(
            arena,
            binder,
            types,
            ContextParts {
                file_name,
                compiler_options,
                options_policy: OptionsPolicy::PRE_RESOLVED,
                // The shared store is installed from the parent below.
                def_store: DefStorePlan::Deferred,
                cache: None,
                cache_order: CacheRestoreOrder::BeforeWarm,
                inherit_has_lib: parent.capabilities.has_lib,
                // Child contexts replace symbol caches with parent snapshots
                // below; starting at zero avoids binder-sized preallocation
                // per child.
                symbol_cache_capacity: Some(0),
            },
        );

        // Propagate parent state that is safe across arenas.
        ctx.no_implicit_override = parent.no_implicit_override;
        ctx.allow_source_file_test_pragmas = parent.allow_source_file_test_pragmas;
        if !parent.lib_contexts.is_empty() {
            ctx.set_lib_contexts_shared(Arc::clone(&parent.lib_contexts));
            ctx.set_actual_lib_file_count(parent.actual_lib_file_count);
        }

        // Share symbol caches: after merge, all binders use global SymbolIds,
        // so SymbolId(N) means the same entity regardless of which arena/binder
        // the child checker operates on. Sharing avoids redundant re-resolution
        // of lib types (Array, Promise, etc.) in each child context.
        ctx.symbol_types = parent.symbol_types.clone();
        ctx.symbol_instance_types = parent.symbol_instance_types.clone();
        ctx.enum_namespace_types = parent.enum_namespace_types.clone();

        // Note: the clone shares (does not copy) the embedded session memo of
        // completed cross-arena delegation results via its internal `Arc`:
        // child writes must be visible to the parent and to sibling children
        // within the same file-check session.
        ctx.lib_delegation_cache = parent.lib_delegation_cache.clone();
        // These per-checker caches are `CowCache`-backed: inheriting the
        // parent's entries is an O(1) `Arc` bump, and the child copy-on-writes
        // only if it actually mutates the inherited map. The historical
        // per-child deep `HashMap::clone` here was O(children × cache-size) —
        // `with_parent_cache` fires 6,735 times per run on the scale-cliff
        // fixtures (issue #13087).
        ctx.namespace_member_resolution_cache
            .borrow_mut()
            .clone_from(&parent.namespace_member_resolution_cache.borrow());
        ctx.export_equals_named_cache
            .borrow_mut()
            .clone_from(&parent.export_equals_named_cache.borrow());
        ctx.nested_namespace_candidates_cache
            .borrow_mut()
            .clone_from(&parent.nested_namespace_candidates_cache.borrow());
        ctx.nested_namespace_candidates_cache_complete =
            Cell::new(parent.nested_namespace_candidates_cache_complete.get());
        ctx.lowering_entity_name_resolution_cache
            .borrow_mut()
            .clone_from(&parent.lowering_entity_name_resolution_cache.borrow());
        ctx.jsdoc_global_typedef_lookup_cache
            .typedef_presence_by_file = Arc::clone(
            &parent
                .jsdoc_global_typedef_lookup_cache
                .typedef_presence_by_file,
        );
        ctx.jsdoc_global_typedef_lookup_cache.tag_presence_by_file = Arc::clone(
            &parent
                .jsdoc_global_typedef_lookup_cache
                .tag_presence_by_file,
        );
        ctx.skip_lib_type_resolution = parent.skip_lib_type_resolution;

        // CRITICAL: Propagate in-progress set from parent to prevent re-entrant
        // heritage merging in child contexts (cross-arena delegation). Without this,
        // child CheckerStates don't see that the parent is already resolving a type,
        // causing unbounded mutual recursion through resolve_lib_type_by_name ↔
        // merge_lib_interface_heritage ↔ build_type_environment chains.
        ctx.lib_heritage_in_progress = parent.lib_heritage_in_progress.clone();

        // Propagate JSDoc typedef re-entrancy state across child checkers.
        // Cross-file JSDoc import/typedef resolution spawns nested CheckerStates;
        // if the active typedef set is reset at that boundary, cyclic CommonJS
        // JSDoc graphs can recurse until stack overflow.
        ctx.jsdoc_typedef_resolving
            .borrow_mut()
            .clone_from(&parent.jsdoc_typedef_resolving.borrow());
        ctx.jsdoc_generic_typedef_resolving
            .borrow_mut()
            .clone_from(&parent.jsdoc_generic_typedef_resolving.borrow());

        // Propagate expando-property resolution state so child checkers do not
        // lose recursion protection while resolving CommonJS/JS property reads
        // across files.
        ctx.expando_property_resolution_set = parent.expando_property_resolution_set.clone();

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
        ctx.share_owner_symbol_type_results = parent.share_owner_symbol_type_results;

        // Every `with_parent_cache` child is a transient delegation checker:
        // the parent consumes only the delegated symbol's type, and no
        // construction site merges the child's diagnostics back — they are
        // dropped at teardown. Mark the whole delegation subtree discarded so
        // none of it pays for diagnostic presentation work (failure
        // elaboration, type display, spelling-suggestion candidate scans).
        ctx.diagnostics_discarded = true;

        ctx
    }

    /// Attributed variant of [`Self::with_parent_cache`].
    ///
    /// Use this for child `CheckerContext` construction sites that do not wrap
    /// the context in `CheckerState::with_parent_cache_attributed`, so
    /// `TSZ_PERF_COUNTERS` can still report the construction reason.
    pub fn with_parent_cache_attributed(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &Self,
        reason: tsz_common::perf_counters::CheckerCreationReason,
    ) -> Self {
        tsz_common::perf_counters::record_with_parent_cache(reason);
        Self::with_parent_cache(arena, binder, types, file_name, compiler_options, parent)
    }
}
