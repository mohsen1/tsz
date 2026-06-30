use crate::diagnostics::SubtypeFailureReason;
use crate::recursion::RecursionResult;
use crate::types::TypeId;

/// Work-budget verdict for one subtype explanation entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExplainFuelState {
    /// Explanation may continue.
    Ready,
    /// Explanation has consumed its per-failure elaboration budget.
    Exhausted,
}

impl ExplainFuelState {
    pub(super) const fn from_fuel(fuel: Option<u32>) -> Self {
        if matches!(fuel, Some(0)) {
            Self::Exhausted
        } else {
            Self::Ready
        }
    }

    pub(super) const fn fallback_reason(
        self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        match self {
            Self::Ready => None,
            Self::Exhausted => Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            }),
        }
    }
}

/// Recursion-entry verdict for subtype explanation elaboration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExplainRecursionEntryState {
    /// The relation pair entered the explanation recursion guard.
    Entered,
    /// The pair hit a cycle/depth/iteration guard and should use the coarse
    /// mismatch fallback.
    Fallback,
}

impl ExplainRecursionEntryState {
    pub(super) const fn from_recursion_result(result: RecursionResult) -> Self {
        match result {
            RecursionResult::Entered => Self::Entered,
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => Self::Fallback,
        }
    }

    pub(super) const fn fallback_reason(
        self,
        source: TypeId,
        target: TypeId,
    ) -> Option<SubtypeFailureReason> {
        match self {
            Self::Entered => None,
            Self::Fallback => Some(SubtypeFailureReason::TypeMismatch {
                source_type: source,
                target_type: target,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::recursion::RecursionResult;
    use crate::types::TypeId;

    use super::{ExplainFuelState, ExplainRecursionEntryState};

    #[test]
    fn fuel_state_reports_exhausted_only_at_zero() {
        assert_eq!(ExplainFuelState::from_fuel(None), ExplainFuelState::Ready);
        assert_eq!(
            ExplainFuelState::from_fuel(Some(1)),
            ExplainFuelState::Ready
        );
        assert_eq!(
            ExplainFuelState::from_fuel(Some(0)),
            ExplainFuelState::Exhausted
        );
    }

    #[test]
    fn exhausted_fuel_builds_type_mismatch_fallback() {
        assert!(
            ExplainFuelState::Ready
                .fallback_reason(TypeId::STRING, TypeId::NUMBER)
                .is_none()
        );
        let reason = ExplainFuelState::Exhausted
            .fallback_reason(TypeId::STRING, TypeId::NUMBER)
            .expect("exhausted fuel should produce a coarse fallback");

        assert!(matches!(
            reason,
            crate::diagnostics::SubtypeFailureReason::TypeMismatch {
                source_type: TypeId::STRING,
                target_type: TypeId::NUMBER,
            }
        ));
    }

    #[test]
    fn recursion_entry_state_preserves_entered_vs_fallback() {
        assert_eq!(
            ExplainRecursionEntryState::from_recursion_result(RecursionResult::Entered),
            ExplainRecursionEntryState::Entered
        );
        for denied in [
            RecursionResult::Cycle,
            RecursionResult::DepthExceeded,
            RecursionResult::IterationExceeded,
        ] {
            assert_eq!(
                ExplainRecursionEntryState::from_recursion_result(denied),
                ExplainRecursionEntryState::Fallback
            );
        }
    }

    #[test]
    fn explain_funnel_uses_named_guard_states() {
        let explain_rs = include_str!("explain.rs");

        assert!(explain_rs.contains("ExplainFuelState::from_fuel"));
        assert!(explain_rs.contains("ExplainRecursionEntryState::from_recursion_result"));
        assert!(!explain_rs.contains("explain_eval_fuel == Some(0)"));
        assert!(!explain_rs.contains("RecursionResult::Cycle"));
    }
}
