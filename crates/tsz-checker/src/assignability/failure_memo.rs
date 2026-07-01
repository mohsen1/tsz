//! Checker-side access to the stamp-guarded assignability failure-analysis
//! memo (issue #13243).
//!
//! A failing TS2322/TS2345 assignment currently executes the
//! reason-collecting relation more than once on identical prepared inputs:
//! once through the `RelationRequest` gateway that picks the diagnostic
//! shape (`assignability_reason_relation_outcome` and friends) and again
//! inside `analyze_assignability_failure` when the error reporter builds the
//! elaboration chain. Both passes run the same configured solver checker
//! over the same `(prepared source, prepared target, solver flags,
//! sound-mode)` key, so the second is a pure re-walk of the most expensive
//! operation in the compiler. These helpers let both call sites share one
//! captured analysis under the [`crate::context::AssignabilityEvalStamp`]
//! validity model: entries are dropped wholesale whenever the session stamp
//! moves, so a hit replays exactly what a fresh pass under the current
//! environment would produce.

use crate::context::{AssignabilityFailureKey, CachedAssignabilityAnalysis};
use crate::state::CheckerState;

impl CheckerState<'_> {
    /// Look up a memoized reason-collecting relation analysis for `key`,
    /// valid for the current session stamp. `None` when the memo is stale,
    /// has no entry, or a type environment is mutably borrowed (re-entrant
    /// call).
    pub(crate) fn failure_memo_lookup(
        &mut self,
        key: AssignabilityFailureKey,
    ) -> Option<CachedAssignabilityAnalysis> {
        let stamp = self.assignability_eval_memo_stamp()?;
        self.ctx
            .type_reference_validation_caches
            .assignability_failure_memo
            .get(stamp, key)
    }

    /// Record a freshly captured reason-collecting relation analysis.
    ///
    /// Mirrors the `evaluate_type_for_assignability` memo's cleanliness
    /// guards: depth/iteration-degraded passes and fuel-exhausted sessions
    /// are never recorded, because a fresher pass must be allowed to improve
    /// on them. The stamp is recomputed after the pass on purpose — the
    /// relation walk can grow the type environments, and the captured
    /// analysis is valid for that *post*-pass state.
    ///
    /// `lazy_failures_at_entry` is a snapshot of `lazy_resolve_failure_count`
    /// taken by the caller immediately before it ran the captured relation. If
    /// the count advanced during the relation, the walk compared against a
    /// `Lazy(DefId)` whose body was not yet registered (`note_lazy_resolve_failure`),
    /// so the analysis is a function of the registration window it ran in, not
    /// of the prepared `(source, target, flags, sound_mode)` key alone. The
    /// failure memo is keyed purely on that prepared key with no
    /// generation/registration guard, so persisting an under-resolved analysis
    /// would let it shadow the correct one once the body registers. This is the
    /// relation-layer analog of the env-eval `unresolved_def_seen` backstop
    /// (issue #12101) and mirrors the suppression
    /// [`publish_shared_constraint_proof`](Self::publish_shared_constraint_proof)
    /// already applies to the cross-file constraint-proof cache.
    ///
    /// Inert today: the eager `ensure_refs_resolved` pre-walk resolves every
    /// referenced `DefId` before a committed relation, so the count does not
    /// advance during a captured relation. The guard becomes load-bearing only
    /// when the on-demand forcing rework drops that pre-walk.
    pub(crate) fn failure_memo_store(
        &mut self,
        key: AssignabilityFailureKey,
        analysis: CachedAssignabilityAnalysis,
        lazy_failures_at_entry: u64,
    ) {
        use crate::state_domain::type_environment::lazy::refs_resolution_fuel_exhausted;

        if analysis.depth_exceeded
            || analysis.iteration_exceeded
            || refs_resolution_fuel_exhausted()
            || self.ctx.eval_session.lazy_resolution_fuel_exhausted()
            || self.ctx.depth_exceeded.get()
            || crate::query_boundaries::common::lazy_resolve_failure_count()
                != lazy_failures_at_entry
        {
            return;
        }
        let Some(stamp) = self.assignability_eval_memo_stamp() else {
            return;
        };
        self.ctx
            .type_reference_validation_caches
            .assignability_failure_memo
            .insert(stamp, key, analysis);
    }
}
