use crate::construction::{TypeDatabase, UnionComplexityCheckpoint};

#[derive(Clone, Copy)]
pub(super) struct ProjectInstantiationCacheLimitSnapshot {
    union_complexity: UnionComplexityCheckpoint,
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
    fn from_ordered_taints<const N: usize>(
        taints: [(bool, ProjectInstantiationCacheTaint); N],
    ) -> Self {
        for (is_tainted, taint) in taints {
            if is_tainted {
                return Self::Unstable(taint);
            }
        }
        Self::Stable
    }

    pub(super) const fn is_stable_for_project_cache(self) -> bool {
        matches!(self, Self::Stable)
    }
}

impl ProjectInstantiationCacheLimitSnapshot {
    pub(super) fn capture(interner: &dyn TypeDatabase) -> Self {
        Self {
            union_complexity: interner.union_complexity_checkpoint(),
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
        let newly_too_complex = interner.union_complexity_changed_since(self.union_complexity);
        let newly_tuple_too_large = interner.is_tuple_too_large() && !self.tuple_too_large;
        let frame_curtailed =
            crate::recursion::solver_frame_bail_count() != self.solver_frame_bail_count;
        ProjectInstantiationCacheStability::from_ordered_taints([
            (
                newly_too_complex,
                ProjectInstantiationCacheTaint::UnionTooComplex,
            ),
            (
                newly_tuple_too_large,
                ProjectInstantiationCacheTaint::TupleTooLarge,
            ),
            (
                frame_curtailed,
                ProjectInstantiationCacheTaint::SolverFrameCurtailment,
            ),
            (
                interner.is_evaluation_fuel_exhausted(),
                ProjectInstantiationCacheTaint::EvaluationFuelExhausted,
            ),
            (
                interner.is_poisoned(),
                ProjectInstantiationCacheTaint::Poisoned,
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProjectInstantiationCacheLimitSnapshot, ProjectInstantiationCacheStability,
        ProjectInstantiationCacheTaint,
    };
    use crate::intern::TypeInterner;

    #[test]
    fn stable_request_state_allows_project_cache_publication() {
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([
            (false, ProjectInstantiationCacheTaint::UnionTooComplex),
            (false, ProjectInstantiationCacheTaint::TupleTooLarge),
            (
                false,
                ProjectInstantiationCacheTaint::SolverFrameCurtailment,
            ),
            (
                false,
                ProjectInstantiationCacheTaint::EvaluationFuelExhausted,
            ),
            (false, ProjectInstantiationCacheTaint::Poisoned),
        ]);

        assert_eq!(stability, ProjectInstantiationCacheStability::Stable);
        assert!(stability.is_stable_for_project_cache());
    }

    #[test]
    fn unstable_request_state_names_limit_reason() {
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([(
            true,
            ProjectInstantiationCacheTaint::SolverFrameCurtailment,
        )]);

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
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([
            (true, ProjectInstantiationCacheTaint::UnionTooComplex),
            (true, ProjectInstantiationCacheTaint::TupleTooLarge),
            (true, ProjectInstantiationCacheTaint::SolverFrameCurtailment),
            (
                true,
                ProjectInstantiationCacheTaint::EvaluationFuelExhausted,
            ),
            (true, ProjectInstantiationCacheTaint::Poisoned),
        ]);

        assert_eq!(
            stability,
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::UnionTooComplex
            )
        );
    }

    #[test]
    fn second_union_event_taints_pre_existing_pending_snapshot() {
        let interner = TypeInterner::new();
        interner.set_union_too_complex();
        let snapshot = ProjectInstantiationCacheLimitSnapshot::capture(&interner);

        assert_eq!(
            snapshot.request_state_stability_after(&interner),
            ProjectInstantiationCacheStability::Stable,
        );
        interner.set_union_too_complex();
        assert_eq!(
            snapshot.request_state_stability_after(&interner),
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::UnionTooComplex,
            ),
        );
        assert!(interner.take_union_too_complex());
    }
}
