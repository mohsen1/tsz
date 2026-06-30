use crate::inference::infer::InferenceContext;
use crate::operations::core::MAX_CONSTRAINT_STEPS;
use crate::operations::{AssignabilityChecker, CallEvaluator, MAX_CONSTRAINT_RECURSION_DEPTH};
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

// Reusable scratch `FxHashSet<TypeId>` for the five `type_contains_placeholder`
// call-sites in this module. Each call previously allocated a fresh set;
// pooling shaves the allocator round-trip plus 2-4 grows. Mirrors the pool
// pattern from #4722 / #4790 / #4801 / #4805 / #4807.
thread_local! {
    static PLACEHOLDER_VISITED_POOL: RefCell<Option<FxHashSet<TypeId>>> =
        const { RefCell::new(None) };
}

#[inline]
pub(super) fn with_placeholder_visited<R>(f: impl FnOnce(&mut FxHashSet<TypeId>) -> R) -> R {
    let mut visited = PLACEHOLDER_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    PLACEHOLDER_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConstraintStepState {
    Continue { next_steps: usize },
    LimitExceeded,
}

pub(super) const fn constraint_step_state(
    current_steps: usize,
    max_steps: usize,
) -> ConstraintStepState {
    if current_steps >= max_steps {
        ConstraintStepState::LimitExceeded
    } else {
        ConstraintStepState::Continue {
            next_steps: current_steps + 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConstraintPairVisitState {
    Entered,
    AlreadyVisited,
}

pub(super) fn constraint_pair_visit_state(
    pairs: &mut FxHashSet<(TypeId, TypeId)>,
    source: TypeId,
    target: TypeId,
) -> ConstraintPairVisitState {
    if pairs.insert((source, target)) {
        ConstraintPairVisitState::Entered
    } else {
        ConstraintPairVisitState::AlreadyVisited
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConstraintDepthState {
    Continue { next_depth: usize },
    LimitExceeded,
}

pub(super) const fn constraint_depth_state(
    current_depth: usize,
    max_depth: usize,
) -> ConstraintDepthState {
    if current_depth >= max_depth {
        ConstraintDepthState::LimitExceeded
    } else {
        ConstraintDepthState::Continue {
            next_depth: current_depth + 1,
        }
    }
}

impl<C: AssignabilityChecker> CallEvaluator<'_, C> {
    /// Structural walker to collect constraints: source <: target.
    pub(crate) fn constrain_types(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        source: TypeId,
        target: TypeId,
        priority: crate::types::InferencePriority,
    ) {
        match constraint_step_state(self.constraint_step_count.get(), MAX_CONSTRAINT_STEPS) {
            ConstraintStepState::Continue { next_steps } => {
                self.constraint_step_count.set(next_steps);
            }
            ConstraintStepState::LimitExceeded => return,
        }

        if matches!(
            constraint_pair_visit_state(&mut self.constraint_pairs.borrow_mut(), source, target),
            ConstraintPairVisitState::AlreadyVisited
        ) {
            return;
        }

        let previous_depth = self.constraint_recursion_depth.get();
        match constraint_depth_state(previous_depth, MAX_CONSTRAINT_RECURSION_DEPTH) {
            ConstraintDepthState::Continue { next_depth } => {
                self.constraint_recursion_depth.set(next_depth);
            }
            ConstraintDepthState::LimitExceeded => return,
        }

        self.constrain_types_impl(ctx, var_map, source, target, priority);
        self.constraint_recursion_depth.set(previous_depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashSet;

    #[test]
    fn constraint_step_state_continues_below_limit_and_stops_at_limit() {
        assert_eq!(
            constraint_step_state(3, 4),
            ConstraintStepState::Continue { next_steps: 4 }
        );
        assert_eq!(
            constraint_step_state(4, 4),
            ConstraintStepState::LimitExceeded
        );
    }

    #[test]
    fn constraint_pair_visit_state_enters_once_then_reports_revisit() {
        let mut pairs = FxHashSet::default();
        assert_eq!(
            constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER),
            ConstraintPairVisitState::Entered
        );
        assert_eq!(
            constraint_pair_visit_state(&mut pairs, TypeId::STRING, TypeId::NUMBER),
            ConstraintPairVisitState::AlreadyVisited
        );
    }

    #[test]
    fn constraint_depth_state_continues_below_limit_and_stops_at_limit() {
        assert_eq!(
            constraint_depth_state(2, 3),
            ConstraintDepthState::Continue { next_depth: 3 }
        );
        assert_eq!(
            constraint_depth_state(3, 3),
            ConstraintDepthState::LimitExceeded
        );
    }
}
