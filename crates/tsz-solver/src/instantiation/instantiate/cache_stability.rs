use crate::construction::TypeDatabase;

#[derive(Clone, Copy)]
pub(super) struct ProjectInstantiationCacheLimitSnapshot {
    union_too_complex: bool,
    tuple_too_large: bool,
    solver_frame_bail_count: u32,
}

/// Whether the surrounding request state permits publishing an instantiation
/// result to the project-wide cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectInstantiationCacheStability {
    Stable,
    Unstable(ProjectInstantiationCacheTaint),
}

/// Which ambient limit signal made the instantiation result unsafe to cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectInstantiationCacheTaint {
    UnionTooComplex,
    TupleTooLarge,
    SolverFrameCurtailment,
    EvaluationFuelExhausted,
    Poisoned,
}

impl ProjectInstantiationCacheStability {
    const fn from_taint_flags(
        newly_too_complex: bool,
        newly_tuple_too_large: bool,
        frame_curtailed: bool,
        evaluation_fuel_exhausted: bool,
        poisoned: bool,
    ) -> Self {
        if newly_too_complex {
            Self::Unstable(ProjectInstantiationCacheTaint::UnionTooComplex)
        } else if newly_tuple_too_large {
            Self::Unstable(ProjectInstantiationCacheTaint::TupleTooLarge)
        } else if frame_curtailed {
            Self::Unstable(ProjectInstantiationCacheTaint::SolverFrameCurtailment)
        } else if evaluation_fuel_exhausted {
            Self::Unstable(ProjectInstantiationCacheTaint::EvaluationFuelExhausted)
        } else if poisoned {
            Self::Unstable(ProjectInstantiationCacheTaint::Poisoned)
        } else {
            Self::Stable
        }
    }

    pub(super) const fn is_stable_for_project_cache(self) -> bool {
        matches!(self, Self::Stable)
    }
}

impl ProjectInstantiationCacheLimitSnapshot {
    pub(super) fn capture(interner: &dyn TypeDatabase) -> Self {
        Self {
            union_too_complex: interner.is_union_too_complex(),
            tuple_too_large: interner.is_tuple_too_large(),
            solver_frame_bail_count: crate::recursion::solver_frame_bail_count(),
        }
    }

    pub(super) fn request_state_stability_after(
        self,
        interner: &dyn TypeDatabase,
    ) -> ProjectInstantiationCacheStability {
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
        ProjectInstantiationCacheStability::from_taint_flags(
            newly_too_complex,
            newly_tuple_too_large,
            frame_curtailed,
            interner.is_evaluation_fuel_exhausted(),
            interner.is_poisoned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ProjectInstantiationCacheStability, ProjectInstantiationCacheTaint};

    #[test]
    fn stable_request_state_allows_project_cache_publication() {
        let stability =
            ProjectInstantiationCacheStability::from_taint_flags(false, false, false, false, false);

        assert_eq!(stability, ProjectInstantiationCacheStability::Stable);
        assert!(stability.is_stable_for_project_cache());
    }

    #[test]
    fn unstable_request_state_names_limit_reason() {
        let stability =
            ProjectInstantiationCacheStability::from_taint_flags(false, false, true, false, false);

        assert_eq!(
            stability,
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::SolverFrameCurtailment
            )
        );
        assert!(!stability.is_stable_for_project_cache());
    }

    #[test]
    fn unstable_request_state_keeps_existing_priority_order() {
        let stability =
            ProjectInstantiationCacheStability::from_taint_flags(true, true, true, true, true);

        assert_eq!(
            stability,
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::UnionTooComplex
            )
        );
    }
}
