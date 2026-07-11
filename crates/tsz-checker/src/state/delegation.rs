//! Cross-arena delegation constructors for [`CheckerState`].
//!
//! Owns the parent-cache child construction family and the Track 10
//! residency-sensitive delegation wiring. See `delegate_for_arena` for the
//! canonical fully-wired delegate construction.

use super::state::CheckerState;
use crate::CheckerContext;
use crate::context::CheckerOptions;
use crate::query_boundaries::common::QueryDatabase;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;

impl<'a> CheckerState<'a> {
    /// Create a child `CheckerState` that shares the parent's caches.
    /// This is used for temporary checkers (e.g., cross-file symbol resolution)
    /// to ensure cache results are not lost (fixes Cache Isolation Bug).
    pub fn with_parent_cache(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a dyn QueryDatabase,
        file_name: String,
        compiler_options: CheckerOptions,
        parent: &Self,
    ) -> Self {
        // Attribution: prefer `with_parent_cache_attributed` at call sites
        // we want to track in the per-reason counter dump (PR #1631).
        // Sites that still call this raw form attribute to
        // `CheckerCreationReason::Other`. See
        // `docs/plan/PERFORMANCE_PLAN.md`.
        Self::with_parent_cache_attributed(
            arena,
            binder,
            types,
            file_name,
            compiler_options,
            parent,
            tsz_common::perf_counters::CheckerCreationReason::Other,
        )
    }

    /// Attributed variant of [`Self::with_parent_cache`]: the caller passes
    /// the reason this child checker is being created so PR #1631's
    /// counter dump can show which call sites drive the construction
    /// explosion. Always prefer this over the raw `with_parent_cache`.
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
        CheckerState {
            ctx: CheckerContext::with_parent_cache(
                arena,
                binder,
                types,
                file_name,
                compiler_options,
                &parent.ctx,
            ),
        }
    }

    /// Construct a fully-wired cross-arena delegate checker (Track 10).
    ///
    /// Single owner of the residency-sensitive delegation pattern: attributed
    /// parent-cache child construction, lib-context inheritance, cross-file
    /// state propagation, and the attributed symbol-target overlay copy.
    /// The transient-delegation `diagnostics_discarded` flag is set once by the
    /// shared `with_parent_cache` constructor (PR #15664), so neither this
    /// factory nor the call site repeats it. Remaining site-specific policy
    /// (delegate `current_file_idx`, resolution-set seeding, perf counters,
    /// depth guards) stays at the call site. Keeping every delegation site on
    /// this factory means the Track 10 migration to structural facts happens in
    /// one place.
    pub(crate) fn delegate_for_arena(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        file_name: String,
        parent: &Self,
        reason: tsz_common::perf_counters::CheckerCreationReason,
    ) -> Box<Self> {
        let mut checker = Box::new(Self::with_parent_cache_attributed(
            arena,
            binder,
            parent.ctx.types,
            file_name,
            parent.ctx.compiler_options.clone(),
            parent,
            reason,
        ));
        checker.ctx.lib_contexts = parent.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&parent.ctx);
        parent
            .ctx
            .copy_symbol_file_targets_to_attributed(&mut checker.ctx, reason);
        checker
    }

    /// Copy resolution-cycle guard sets from `parent` into this child checker and
    /// wire up the shared `DefinitionStore`. Call immediately after constructing a
    /// class-delegation child checker via `with_parent_cache_attributed`.
    pub(super) fn propagate_class_delegation_setup(
        &mut self,
        parent: &CheckerState,
        skip_sym: tsz_binder::SymbolId,
    ) {
        for &id in &parent.ctx.class_instance_resolution_set {
            self.ctx.class_instance_resolution_set.insert(id);
        }
        for &id in &parent.ctx.symbol_resolution_set {
            if id != skip_sym {
                self.ctx.symbol_resolution_set.insert(id);
            }
        }
        for &id in &parent.ctx.class_constructor_resolution_set {
            self.ctx.class_constructor_resolution_set.insert(id);
        }
        self.ctx.ensure_both_envs_have_definition_store();
    }
}
