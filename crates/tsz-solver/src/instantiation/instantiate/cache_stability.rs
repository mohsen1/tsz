use crate::construction::TypeDatabase;

#[derive(Clone, Copy)]
pub(super) struct ProjectInstantiationCacheLimitSnapshot {
    union_too_complex: bool,
    tuple_too_large: bool,
    solver_frame_bail_count: u32,
}

impl ProjectInstantiationCacheLimitSnapshot {
    pub(super) fn capture(interner: &dyn TypeDatabase) -> Self {
        Self {
            union_too_complex: interner.is_union_too_complex(),
            tuple_too_large: interner.is_tuple_too_large(),
            solver_frame_bail_count: crate::recursion::solver_frame_bail_count(),
        }
    }

    pub(super) fn request_state_is_stable_after(self, interner: &dyn TypeDatabase) -> bool {
        // Limit signals, each a reason a result is bounded/degraded:
        //  - union_too_complex (TS2590) / tuple_too_large (TS2799): sticky flags
        //    a nested `evaluate_*` can trip; gate on newly-tripped so a
        //    pre-existing sibling flag does not block an unrelated result.
        //  - solver-frame curtailment: a nested `evaluate_*` can return an
        //    under-evaluated form without flipping the instantiator's own
        //    `depth_exceeded`; the monotonic counter changing detects it.
        let newly_too_complex = interner.is_union_too_complex() && !self.union_too_complex;
        let newly_tuple_too_large = interner.is_tuple_too_large() && !self.tuple_too_large;
        let frame_curtailed =
            crate::recursion::solver_frame_bail_count() != self.solver_frame_bail_count;
        !newly_too_complex
            && !newly_tuple_too_large
            && !frame_curtailed
            && !interner.is_evaluation_fuel_exhausted()
            && !interner.is_poisoned()
    }
}
