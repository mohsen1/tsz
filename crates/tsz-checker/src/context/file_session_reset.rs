//! T2.1 file-session reset boundary.
//!
//! Per `PERFORMANCE_PLAN.md` §6 step 5 ("Introduce `FileSession` reset
//! boundaries after fields are classified"), this module implements the
//! `reset_for_next_file()` helper on `CheckerContext` that clears the
//! file-local state most likely to leak across file boundaries.
//!
//! Scope of this first pass: the critical subset shown in plan §6's
//! illustrative impl — file-keyed diagnostic buffers, node-keyed
//! request/class caches, resolution-stack debug invariants, and the
//! speculative depth counters that gate recursion.
//!
//! Retained caches are retained only when their ownership invariant is explicit:
//! program-stable lib/type caches are checker orchestration state, query-boundary
//! caches carry an explicit semantic request shape, and solver caches are owned
//! by solver data structures. Caches keyed by `NodeIndex`, `FlowNodeId`, or
//! binder-local `SymbolId` must be cleared or rebuilt when the active file or
//! binder changes. See `docs/architecture/CHECKER_CONTEXT_CACHE_OWNERSHIP.md`
//! for the field-level inventory.
//!
//! This helper is **not yet called from anywhere** — it exists as the
//! boundary API so that the future T2.1.B "sequential session-reuse
//! path behind a flag" PR can wire it into the per-file loop without
//! also designing the reset semantics at the same time.

use super::CheckerContext;
use crate::control_flow::FlowGraph;
use crate::query_boundaries::common::TypeEnvironment;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;

impl<'a> CheckerContext<'a> {
    /// Reset file-local state so the same `CheckerContext` can be
    /// reused for the next file in a sequential session-reuse path.
    ///
    /// Clears or resets the fields the plan §6 marks as having the
    /// highest cross-file leak risk:
    ///
    /// - **Diagnostic buffers** (`DiagnosticsOnly` class): diagnostics
    ///   collected during this file's check would otherwise spill into
    ///   the next file's diagnostic stream.
    /// - **Position-keyed `emitted_diagnostics`** set: positions are
    ///   file-local indices, so retaining them would suppress a
    ///   genuine duplicate in the next file.
    /// - **`NodeIndex`-keyed caches** (`request_node_types`,
    ///   `class_instance_type_cache`, `class_constructor_type_cache`):
    ///   raw `NodeIndex` collides across files; carrying entries
    ///   would return one file's type for another file's node.
    /// - **Resolution stacks** (`node_resolution_stack`,
    ///   `import_resolution_stack`): non-empty stacks would create
    ///   false-recursion diagnostics in the next file. Symbol-
    ///   resolution stack/set are `debug_assert!`'d empty rather
    ///   than force-cleared, because a non-empty state at file
    ///   boundary indicates a programming error in the prior file's
    ///   check, not a value worth silently discarding.
    /// - **Implicit-any closure sets**: keyed by node id, would
    ///   suppress or replay errors in the wrong file.
    /// - **Class-checking sets** (`checking_classes`,
    ///   `checked_classes`): retain state would cause false
    ///   "already checked" decisions in the next file.
    /// - **Pending-circular-return sites**: contains `NodeIndex`
    ///   values that collide across files.
    /// - **No-overload call nodes**: keyed by node id; retaining
    ///   would mis-flag calls in the next file.
    /// - **Depth counters** (`call_depth`, `circ_ref_depth`,
    ///   `overlap_depth`, `recursion_depth`, `instantiation_depth`):
    ///   non-zero depth at file boundary would suppress legitimate
    ///   recursion in the next file or trip TS2589-like behaviour.
    /// - **Module thread-local memoisations** in
    ///   `types::utilities::{cycle_guard, enum_utils, const_enum_eval}`:
    ///   each is keyed by `NodeIndex` and must be cleared when
    ///   reusing a worker across files.
    ///
    /// Fields not cleared in this pass (and why):
    ///
    /// - `SymbolId`-keyed caches (`symbol_types`,
    ///   `symbol_instance_types`, `lib_delegation_cache`, etc.):
    ///   stable symbol identity makes these correct to retain.
    /// - `Atom`/string-keyed lib caches: stable across compilations.
    /// - The bulk of the 119 `FileLocalReset` manifest entries:
    ///   purely-keyed caches whose retained entries are
    ///   correctness-neutral and merely costs a re-fetch. They will
    ///   be added here only if attribution data shows cold-start
    ///   cost matters.
    ///
    /// # Speculation
    ///
    /// This helper does **not** handle speculative rollback. The
    /// `SpeculationScoped` lifetime class is rolled back by its own
    /// save/restore mechanism scoped to overload/generic checking;
    /// this reset is for *successful* file completion only.
    pub fn reset_for_next_file(&mut self) {
        // Attribution counter: increments only on the sequential session-
        // reuse path (T2.1.B). Zero on the default construction-per-file
        // path, so reuse vs construct is observable from a single counter.
        // The helper gates on `enabled_fast()` once before the
        // `counters()` `OnceLock` deref, so disabled runs pay only one
        // relaxed atomic load + branch.
        tsz_common::perf_counters::record_file_session_reset();

        // Diagnostic buffers.
        self.diagnostics.clear();
        self.emitted_diagnostics.clear();
        self.callback_return_type_errors.clear();
        self.modules_with_ts2307_emitted.clear();
        self.deferred_truthiness_diagnostics.clear();
        self.deferred_jsx_import_source_error = None;
        self.jsx_import_source_checked = false;

        // Primary NodeIndex→TypeId cache. This is the cache that holds
        // the type of every checked AST node; without clearing it, a
        // switch to a new arena would return the prior file's types
        // for the new file's identically-numbered nodes — producing
        // silent-but-wrong diagnostics (often *zero* diagnostics
        // because the cached "fine" type wins).
        self.node_types.clear();

        // Node-keyed caches (FxHashMap shape).
        self.request_node_types.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();
        self.class_instance_type_to_decl.clear();
        self.flow_narrowed_nodes.clear();
        self.daa_error_nodes.clear();
        self.deferred_ts2454_errors.clear();
        self.type_only_nodes.clear();
        self.closures_with_contextual_this_type.clear();
        self.jsdoc_typedef_anchor_pos.set(u32::MAX);
        // Object-literal diagnostic elaboration state. All three
        // fields are file-local: `property_diag_targets` and
        // `contextual_targets` are `FxHashMap<NodeIndex, TypeId>`,
        // and `partial_initializers` is a stack of in-progress
        // object-literal initializers. Without clearing, the next
        // file's TS2322/TS2345 elaboration loses the nested
        // sub-messages ("Types of property 'X' are incompatible.")
        // because the prior file's `contextual_targets` returns
        // the wrong target type for the new file's identically-
        // numbered nodes — observed as ~260 missing elaboration
        // lines on monorepo-001 (#5643).
        self.object_literal_tracking.property_diag_targets.clear();
        self.object_literal_tracking.contextual_targets.clear();
        self.object_literal_tracking.partial_initializers.clear();
        // `name_resolution_diagnostics.reported_nodes` holds the
        // `NodeIndex` set per file for TS2304/TS2552 dedup; clear it
        // alongside the counter.
        self.name_resolution_diagnostics.reported_nodes.clear();
        self.name_resolution_diagnostics
            .spelling_suggestions_emitted
            .set(0);

        // Class chain / heritage caches keyed by `NodeIndex`.
        self.class_chain_summary_cache.borrow_mut().clear();
        self.class_symbol_to_decl_cache.borrow_mut().clear();
        self.heritage_symbol_cache.borrow_mut().clear();
        self.base_constructor_expr_cache.borrow_mut().clear();
        self.base_instance_expr_cache.borrow_mut().clear();
        self.class_decl_miss_cache.borrow_mut().clear();
        self.jsx_intrinsic_props_cache.clear();
        self.jsx_namespace_symbol_cache = None;
        self.jsx_intrinsic_elements_symbol_cache = None;
        self.jsx_intrinsic_elements_type_cache = None;

        // Flow-analysis state (`FlowNodeId` and `(u32, u32)` position
        // keyed). All file-local; carrying entries across files yields
        // wrong narrowing.
        self.flow_analysis_cache.borrow_mut().clear();
        self.flow_worklist.borrow_mut().clear();
        self.flow_in_worklist.borrow_mut().clear();
        self.flow_visited.borrow_mut().clear();
        self.flow_results.borrow_mut().clear();
        self.flow_switch_reference_cache.borrow_mut().clear();
        self.flow_numeric_atom_cache.borrow_mut().clear();
        self.flow_reference_match_cache.borrow_mut().clear();
        self.symbol_last_assignment_pos.borrow_mut().clear();
        self.symbol_flow_confirmed.borrow_mut().clear();
        self.emitted_ts2454_errors.clear();
        // `CallPredicateMap` has no `.clear()`; replace with default.
        // `NarrowableIdentifierCache` is a `Vec<u8>`-backed dense cache;
        // replace with an empty one to drop the stored data without
        // exposing an internal `.clear()` method through the public API.
        self.call_type_predicates = crate::control_flow::CallPredicateMap::default();
        *self.narrowable_identifier_cache.borrow_mut() =
            crate::context::NarrowableIdentifierCache::new();

        // Resolution stacks (force-clear the import stack; symbol-
        // resolution stack/set are asserted empty as an invariant).
        self.node_resolution_stack.clear();
        self.import_resolution_stack.clear();
        self.typeof_resolution_stack.borrow_mut().clear();
        self.symbol_resolution_depth.set(0);

        // Implicit-any tracking sets.
        self.implicit_any_checked_closures.clear();
        self.implicit_any_contextual_closures.clear();
        self.deferred_implicit_any_closures.clear();
        self.speculative_implicit_any_closures.clear();
        self.pending_implicit_any_vars.clear();
        self.reported_implicit_any_vars.clear();
        self.deferred_excess_property_implicit_any_diagnostics
            .clear();

        // Class checking state.
        self.checking_classes.clear();
        self.checked_classes.clear();
        self.class_instance_resolution_set.clear();
        self.class_constructor_resolution_set.clear();
        self.circular_class_symbols.clear();
        self.inheritance_graph.clear();

        // Pending-circular-return sites + no-overload call nodes.
        self.pending_circular_return_sites.clear();
        self.no_overload_call_nodes.clear();
        self.non_closure_circular_return_tracking_depth = 0;

        // Symbol/circularity state whose keys or values are file-local
        // `SymbolId`s, plus string-name guards that are meaningful only inside
        // the current file's resolution session.
        self.circular_type_aliases.clear();
        self.import_conflict_names.clear();
        self.module_namespace_resolution_set.clear();
        self.import_type_alias_types.clear();
        self.jsdoc_enum_resolution_set.clear();
        self.symbol_dependency_stack.clear();
        self.symbol_dependencies.clear();
        self.referenced_symbols.borrow_mut().clear();
        self.written_symbols.borrow_mut().clear();
        self.referenced_as_property.borrow_mut().clear();
        self.destructured_bindings.clear();
        self.next_binding_group_id = 0;
        self.destructured_binding_sources.clear();

        // Per-file type/evaluation state. `build_type_environment()` rebuilds
        // these for the next file before statement checking.
        self.type_environment.replace(TypeEnvironment::new());
        self.type_env.replace(TypeEnvironment::new());
        self.application_eval_set.clear();
        self.mapped_eval_set.clear();
        self.type_resolution_visiting.clear();
        self.pruning_union_members = false;
        self.jsdoc_typedef_resolving.borrow_mut().clear();
        self.resolving_jsdoc_typedefs.borrow_mut().clear();
        self.refs_resolved.clear();
        self.application_symbols_resolved.clear();
        self.application_symbols_resolution_set.clear();
        self.namespace_module_names.clear();
        self.clear_env_eval_cache();

        // Depth counters: reset to their base depth and clear the
        // `exceeded` flag.
        self.call_depth.borrow_mut().reset();
        self.circ_ref_depth.borrow_mut().reset();
        self.overlap_depth.borrow_mut().reset();
        self.recursion_depth.borrow_mut().reset();
        self.instantiation_depth.set(0);
        self.depth_exceeded.set(false);
        self.relation_depth_exceeded.set(false);
        self.skip_callable_type_param_suppression.set(false);
        self.heritage_merge_depth.set(0);
        self.type_resolution_fuel
            .set(crate::state::MAX_TYPE_RESOLUTION_OPS);

        // Traversal/context stacks and transient flags should be empty between
        // source files, matching a freshly constructed checker.
        self.type_parameter_scope.clear();
        self.in_conditional_extends_depth = 0;
        self.typeof_param_scope.clear();
        self.type_param_constraint_excluded_params.clear();
        self.contextual_type = None;
        self.contextual_type_is_assertion = false;
        self.is_checking_statements = false;
        self.is_in_ambient_declaration_file = false;
        self.in_destructuring_target = false;
        self.preserve_destructuring_initializer_overload_diagnostics = false;
        self.skip_flow_narrowing = false;
        self.return_type_stack.clear();
        self.yield_type_stack.clear();
        self.generator_next_type_stack.clear();
        self.generator_yield_operand_types.clear();
        self.generator_had_ts7057 = false;
        self.this_type_stack.clear();
        self.function_owned_this_stack.clear();
        self.enclosing_class = None;
        self.enclosing_class_chain.clear();
        self.async_depth = 0;
        self.inside_closure_depth = 0;
        self.in_const_assertion = false;
        self.preserve_literal_types = false;
        self.use_declared_type_for_identifier = false;
        self.skip_array_contextual_supertype_collapse = false;
        self.generic_excess_skip = None;
        self.iteration_depth = 0;
        self.switch_depth = 0;
        self.function_depth = 0;
        self.is_unreachable = false;
        self.has_reported_unreachable = false;
        self.label_stack.clear();
        self.had_outer_loop = false;
        self.suppress_definite_assignment_errors = false;
        self.js_body_uses_arguments = false;

        // Module/export synthesis state is derived from the active file and
        // may contain file-local symbol identities in its values.
        self.js_export_surface_cache.clear();
        self.js_export_surface_resolution_set.clear();
        self.expando_property_resolution_set.clear();
        self.module_augmentation_value_decls.clear();
        self.module_augmentation_application_set
            .borrow_mut()
            .clear();
        self.emitted_ts2411_for_iface_prop.clear();

        // Module-scoped thread-local memoisations that key by file-
        // local `NodeIndex`.
        crate::types_domain::utilities::cycle_guard::clear_visited_sets();
        crate::types_domain::utilities::enum_utils::clear_enum_eval_memo();
        crate::types_domain::utilities::const_enum_eval::clear_const_eval_memo();

        // Invariants: these stacks must be empty at the file
        // boundary. A non-empty state indicates a logic bug in the
        // prior file's check (missing pop, early return inside a
        // resolution scope). Force-clearing would mask that bug.
        debug_assert!(
            self.symbol_resolution_stack.is_empty(),
            "symbol_resolution_stack non-empty at file boundary",
        );
        debug_assert!(
            self.symbol_resolution_set.is_empty(),
            "symbol_resolution_set non-empty at file boundary",
        );
    }

    /// Re-target this `CheckerContext` at the next file in a sequential
    /// session-reuse path (`PERFORMANCE_PLAN.md` §6 step 5; T2.1.B).
    ///
    /// Steps, in order:
    /// 1. Run `reset_for_next_file()` to drain file-local state from the
    ///    previous file (diagnostics, node-keyed caches, depth counters,
    ///    resolution stacks; see that method's docstring for the full
    ///    list).
    /// 2. Swap the borrowed `arena` and `binder` references to point at the
    ///    next file. Both borrows must originate from the same enclosing
    ///    `'a` lifetime — typically the `program` lifetime in
    ///    `crates/tsz-cli/src/driver/check.rs`, where every file's
    ///    `arena` and pre-built `BinderState` are guaranteed to outlive
    ///    the whole sequential loop.
    /// 3. Update `current_file_idx` and `file_name` so per-file
    ///    diagnostic anchoring and arena lookups land on the right
    ///    file.
    ///
    /// Per-file configuration (compiler-option flags, `file_is_esm`,
    /// `resolved_modules`, parse-error positions) is **not** touched
    /// here — that's the job of the caller's existing
    /// `configure_checker_per_file` (in the driver) or
    /// `set_resolved_modules` / `set_current_file_idx` (in tests). Keep
    /// that responsibility outside this helper so the API surface
    /// stays narrow: this method moves the *checker* to the next file;
    /// the caller moves the *configuration*.
    ///
    /// Cross-file program state — `lib_contexts`, `all_arenas`,
    /// `all_binders`, the shared `DefinitionStore`, symbol-keyed
    /// caches that are stable across files — is intentionally
    /// **preserved**. Those entries are exactly the allocations
    /// session-reuse is meant to amortize.
    ///
    /// # Soundness of swapping `&'a` fields
    ///
    /// `arena` and `binder` are `&'a` references. Reassigning a `&'a T`
    /// field to a different `&'a T` is type-safe in Rust as long as
    /// both references carry the same `'a` — `'a` is fixed once the
    /// `CheckerContext` is constructed. The caller's contract is that
    /// every file's `NodeArena` and `BinderState` outlives the
    /// `CheckerContext`. Pre-building all binders into a
    /// `Vec<BinderState>` before the loop satisfies this naturally
    /// because `program.files[i].arena` and `binders[i]` both live
    /// for the duration of the function that owns the `Vec`.
    pub fn switch_to_file(
        &mut self,
        arena: &'a NodeArena,
        binder: &'a BinderState,
        file_name: String,
        file_idx: usize,
    ) {
        self.reset_for_next_file();
        // SymbolId-keyed caches that the plan §6 line 513-519
        // *claims* are "safe across files, assuming stable symbol
        // identity". That claim is correct for **parent → child**
        // checker construction within one session (the parent's
        // already-populated cache is propagated to the child via
        // `with_parent_cache` — the child sees the parent's resolved
        // SymbolId(N) entries, and the SymbolId namespace is the
        // parent's).
        //
        // The claim is **wrong for switching to a new binder**: each
        // per-file `BinderState` allocates SymbolIds starting from 0
        // (no `base_offset` in production binder construction). So
        // `SymbolId(N)` in the prior file's binder refers to a
        // different symbol than `SymbolId(N)` in the next file's
        // binder. Holding the prior file's `symbol_types[N]` across
        // the swap would return the prior file's TypeId for the next
        // file's symbol — exactly the divergence observed in the
        // T2.1.B driver wire-up PR (#5643) on monorepo-001 where
        // the reuse path emitted 22% extra diagnostics for
        // mismatched `Leaf<N>` references.
        //
        // Clear the SymbolId-keyed caches on every `switch_to_file`.
        // The cost is a re-fetch of common symbols (`Array`,
        // `Promise`, etc.) at the start of the next file's check,
        // which is exactly the cost the default construction-per-file
        // path also pays. The reuse path's win comes from amortising
        // `apply_to`, `with_options_deferred_def_store`, and the
        // shared `QueryCache` — not from carrying symbol_types across
        // files.
        self.symbol_types = crate::context::SymbolTypeCache::with_capacity(binder.symbols.len());
        self.symbol_instance_types =
            crate::context::SymbolTypeCache::with_capacity(binder.symbols.len());
        self.enum_namespace_types.clear();
        self.lib_delegation_cache.clear();
        self.var_decl_types.clear();
        self.merged_value_types.clear();
        // SymbolId↔DefId mapping caches. The forward map is keyed
        // by SymbolId (file-local namespace); the reverse map is
        // keyed by DefId (globally stable) but its **values** are
        // SymbolIds from the prior file's binder. Carrying either
        // across a binder swap makes `get_or_create_def_id(sym_id)`
        // return the prior file's DefId for an unrelated symbol.
        //
        // Clearing `def_to_symbol` is *also* required even though
        // its key is stable: a `DefId` registered against the prior
        // file's `SymbolId(N)` will, after the swap, decode as
        // `SymbolId(N)` in the new file's binder — which is a
        // different symbol. Downstream lookups (`def_to_symbol_id`
        // in error reporting, namespace exports) would resolve to
        // the wrong file's symbol.
        self.symbol_to_def.borrow_mut().clear();
        self.def_to_symbol.borrow_mut().clear();
        // String-keyed caches whose **values** carry per-file `SymbolId`
        // or `DefId` references. The keys are program-stable identifier
        // names (`"Leaf5"`, `"Promise"`), which made them look safe at
        // first glance — but a String key with file-local values is
        // still file-local in effect. Carrying entries across a
        // `switch_to_file` makes `lookup_by_name("Leaf5")` return the
        // prior file's binder SymbolIds, which decode against the new
        // file's binder as unrelated symbols.
        //
        // This was the source of the residual TS2820 divergence in
        // `#5643` after `#5683`'s SymbolId-keyed clears: monorepo-001
        // emitted spelling suggestions like `"leaf-5" is not assignable
        // to "leaf-4"` (flag-off) vs. `"leaf-4" → "leaf-2"` (flag-on)
        // at the same source position — the inferred *target type*
        // differed because `Leaf5` resolved through stale cached
        // SymbolIds to a different file's interface shape.
        self.symbol_name_candidates_cache.borrow_mut().clear();
        self.lowering_entity_name_resolution_cache
            .borrow_mut()
            .clear();
        self.namespace_exports_cache.borrow_mut().clear();
        // `def_type_params` and `def_no_type_params` are keyed by
        // globally-stable `DefId`. The values are program-stable
        // type-param info (interned `Atom` names, solver `TypeId`
        // constraints/defaults). Safe to keep — and clearing them
        // would force a re-fetch from `TypeEnvironment` /
        // `DefinitionStore` on every cross-file lookup.
        //
        // Reset the warm-once gate so the next
        // `warm_local_caches_from_shared_store` call actually does
        // work. Without this reset, the call below is a no-op
        // (the gate short-circuits) and the cleared
        // `symbol_to_def`/`def_to_symbol` maps stay empty — every
        // subsequent `get_or_create_def_id` call would fall back
        // to creating a fresh DefId, fragmenting the type universe.
        self.local_caches_warmed.set(false);
        // `lib_type_resolution_cache` is keyed by `String` (lib type
        // names), which is program-stable, NOT file-local. Keep it.
        // `shared_lib_type_cache` is `Arc`-shared at construction
        // time; never overwritten by the reset.
        self.arena = arena;
        self.binder = binder;
        self.file_name = file_name;
        self.current_file_idx = file_idx;
        self.flow_graph = Some(FlowGraph::new(&binder.flow_nodes));
        // Re-warm SymbolId-keyed caches from the shared
        // `DefinitionStore`. `ProgramContext::apply_to` calls this
        // helper once at construction; we just emptied the caches
        // it warmed, so we have to call it again to repopulate the
        // entries the new file's check assumes are present (e.g.
        // pre-resolved DefId→SymbolId mappings for cross-file
        // references). Without this, the next file's check
        // misses diagnostics for symbols whose `SymbolTypeCache`
        // entry was populated upstream and is now gone — observed
        // as ~16% missing diagnostics on monorepo-001 when the
        // re-warm was forgotten.
        self.warm_local_caches_from_shared_store();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use tsz_binder::BinderState;
    use tsz_parser::parser::NodeArena;
    use tsz_solver::TypeInterner;

    fn fresh_ctx<'a>(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
    ) -> CheckerContext<'a> {
        CheckerContext::new(
            arena,
            binder,
            types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        )
    }

    #[test]
    fn reset_clears_diagnostic_buffers_and_node_keyed_caches() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Populate via direct field access (we control the test).
        ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
            "test.ts".to_string(),
            0,
            1,
            "test".to_string(),
            0,
        ));
        ctx.emitted_diagnostics.insert((0, 1));
        ctx.instantiation_depth.set(7);

        assert_eq!(ctx.diagnostics.len(), 1);
        assert_eq!(ctx.emitted_diagnostics.len(), 1);
        assert_eq!(ctx.instantiation_depth.get(), 7);

        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert!(ctx.emitted_diagnostics.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }

    #[test]
    fn reset_is_idempotent() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        ctx.reset_for_next_file();
        ctx.reset_for_next_file();

        assert!(ctx.diagnostics.is_empty());
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }

    #[test]
    fn reset_clears_all_recursion_depth_counters() {
        // The reset helper resets five depth counters: four
        // `RefCell<DepthCounter>` (call/circ_ref/overlap/recursion) plus
        // one `Cell<u32>` (instantiation). The original "diagnostic
        // buffers" test only exercises `instantiation_depth`. This test
        // locks the semantics of the four RefCell-backed counters,
        // including the sticky `exceeded` flag that a careless future
        // refactor (e.g. clearing only `depth` and forgetting `exceeded`)
        // would silently break — and a non-cleared `exceeded` would
        // suppress legitimate TS2589-style depth errors in the next
        // file checked on the reused context.
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut ctx = fresh_ctx(&arena, &binder, &types);

        // Drive each counter past zero and set the sticky exceeded flag.
        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let mut d = depth_cell.borrow_mut();
            assert!(d.enter(), "enter should succeed under max_depth");
            assert!(d.enter(), "second enter should succeed");
            d.mark_exceeded();
            assert_eq!(d.depth(), 2);
            assert!(d.is_exceeded());
        }
        ctx.instantiation_depth.set(11);

        ctx.reset_for_next_file();

        for depth_cell in [
            &ctx.call_depth,
            &ctx.circ_ref_depth,
            &ctx.overlap_depth,
            &ctx.recursion_depth,
        ] {
            let d = depth_cell.borrow();
            assert_eq!(d.depth(), 0, "depth not cleared on reset");
            assert!(
                !d.is_exceeded(),
                "exceeded flag not cleared on reset — would silently \
                 suppress real depth errors in the next file",
            );
        }
        assert_eq!(ctx.instantiation_depth.get(), 0);
    }
}
