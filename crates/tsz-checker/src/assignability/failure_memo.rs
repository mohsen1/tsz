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
    pub(crate) fn failure_memo_store(
        &mut self,
        key: AssignabilityFailureKey,
        analysis: CachedAssignabilityAnalysis,
    ) {
        use crate::state_domain::type_environment::lazy::{
            global_resolution_fuel_exhausted, refs_resolution_fuel_exhausted,
        };

        if analysis.depth_exceeded
            || analysis.iteration_exceeded
            || refs_resolution_fuel_exhausted()
            || global_resolution_fuel_exhausted()
            || self.ctx.depth_exceeded.get()
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
