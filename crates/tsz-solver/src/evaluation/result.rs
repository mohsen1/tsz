//! Typed result boundary for type evaluation.
//!
//! The evaluator still emits a single `TypeId` today, but this wrapper names
//! the result stage so future cache/provenance metadata can be attached
//! without threading loose tuples through the evaluation engine.
//!
//! # Termination channel (#14346 scaffold)
//!
//! Beyond the evaluated `TypeId`, the result now carries an explicit
//! [`Termination`] verdict mirroring the `InstantiationResult::overflowed`
//! precedent (`crate::instantiation::result`). The verdict names *whether* a
//! bound (depth, fuel, solver-stack frame budget, cross-eval cycle, query-op
//! budget) cut the walk short and, if so, which one — instead of letting an
//! outer collapse silently treat a budget-truncated partial as a finished
//! type.
//!
//! In this slice the channel is a parity-safe scaffold: the sole producer
//! ([`crate::evaluation::evaluate::TypeEvaluator::evaluate_request_result`])
//! always reports [`Termination::Complete`], and every consumer collapses
//! through [`EvaluationResult::into_type_id`], which ignores the verdict. The
//! [`EvaluationResult::incomplete`] constructor is intentionally unreached
//! (dead-code) here — a future stage wires the bail sites to it so a
//! limit-truncated walk stops fabricating a wrong, fully-formed type.

use crate::types::TypeId;

/// Which evaluation guard cut a walk short.
///
/// One discriminant per bail class in
/// `crate::evaluation::evaluate::TypeEvaluator::evaluate`: the
/// recursion-depth guard, the process-wide evaluation fuel counter, the shared
/// solver-stack-frame breaker, the cross-evaluator global-depth cycle limit,
/// and the per-query operation budget. Carried inside
/// [`Termination::Incomplete`] so a downstream consumer can distinguish a
/// genuine result from a budget-limited approximation, and (in a later stage)
/// refuse to memoize it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationKind {
    /// The per-evaluator recursion-depth guard tripped (`guard.is_exceeded`).
    DepthExceeded,
    /// The process-wide evaluation fuel counter was exhausted.
    FuelExhausted,
    /// The shared cross-operation solver-stack-frame breaker bailed
    /// (`crate::recursion::with_solver_frame`).
    SolverStackFrames,
    /// The cross-evaluator global-depth limit (`MAX_GLOBAL_EVAL_DEPTH`)
    /// short-circuited to keep the native stack bounded.
    CrossEvalCycle,
    /// The per-query operation budget (`enter_eval_query_budget`) ran out.
    QueryOpBudget,
}

/// Whether an evaluation walk ran to completion or was bounded short.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Termination {
    /// The walk finished within every guard; `type_id` is the true result.
    Complete,
    /// A guard ([`TerminationKind`]) cut the walk short. `partial` is the
    /// relation-preserving approximation the bail surfaced (never a sentinel
    /// leak), kept so a depth-unaware consumer does not fall back to an
    /// un-evaluated original.
    Incomplete {
        kind: TerminationKind,
        partial: TypeId,
    },
}

/// The normalized output of an evaluation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationResult {
    type_id: TypeId,
    termination: Termination,
}

impl EvaluationResult {
    /// Construct a result for a walk that ran to completion.
    pub const fn complete(type_id: TypeId) -> Self {
        Self {
            type_id,
            termination: Termination::Complete,
        }
    }

    /// Construct a result for a walk a guard cut short, carrying the
    /// relation-preserving `partial` type and the [`TerminationKind`] that
    /// fired.
    ///
    /// Intentionally unreached in this scaffold slice: the sole producer
    /// always reports [`Termination::Complete`], so behavior is byte-identical
    /// to before. A future stage routes the bail sites here.
    pub const fn incomplete(partial: TypeId, kind: TerminationKind) -> Self {
        Self {
            type_id: partial,
            termination: Termination::Incomplete { kind, partial },
        }
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    /// The termination verdict for this walk.
    pub const fn termination(self) -> Termination {
        self.termination
    }

    /// Whether a guard cut this walk short.
    pub const fn is_incomplete(self) -> bool {
        matches!(self.termination, Termination::Incomplete { .. })
    }

    /// Collapse the result to a single `TypeId`, ignoring the termination
    /// verdict. The verdict-aware path is reserved for a future stage; today
    /// every consumer collapses here, so the emitted type and diagnostics are
    /// byte-identical to the pre-channel evaluator.
    pub const fn into_type_id(self) -> TypeId {
        self.type_id
    }

    pub fn is_identity_for(self, input: TypeId) -> bool {
        self.type_id == input
    }
}

#[cfg(test)]
mod tests {
    use super::{EvaluationResult, Termination, TerminationKind};
    use crate::types::TypeId;

    #[test]
    fn complete_result_wraps_evaluated_type_id() {
        let result = EvaluationResult::complete(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert_eq!(result.termination(), Termination::Complete);
        assert!(!result.is_incomplete());
        assert!(result.is_identity_for(TypeId::STRING));
        assert!(!result.is_identity_for(TypeId::NUMBER));
    }

    #[test]
    fn incomplete_result_carries_partial_and_kind() {
        // Reserved for a future stage; today no producer reaches this arm.
        let result = EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);

        assert!(result.is_incomplete());
        // `into_type_id` returns the relation-preserving partial regardless of
        // the verdict — the same collapse every consumer performs today.
        assert_eq!(result.into_type_id(), TypeId::NUMBER);
        assert_eq!(
            result.termination(),
            Termination::Incomplete {
                kind: TerminationKind::DepthExceeded,
                partial: TypeId::NUMBER,
            }
        );
    }
}
