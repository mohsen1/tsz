//! Cross-evaluator expansion guard (#11586).
//!
//! Infer-pattern matching and the subtype checker expand
//! `Application`/`Mapped`/conditional types by spinning up *fresh*
//! [`TypeEvaluator`](super::evaluate::TypeEvaluator)s whose per-instance
//! recursion guard, depth counter, and result cache all start empty. The
//! helpers that drive that expansion only hold `&self`, so they cannot reuse the
//! current evaluator's `&mut` evaluation path and must construct a new evaluator
//! instead.
//!
//! A recursive conditional/`infer` utility applied to a literal/object/tuple
//! argument (`type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T;`,
//! `Awaited`, Zod-style wrappers, …) re-enters the *same* `TypeId` through a new
//! evaluator at every level, so no per-instance guard ever fires. The recursion
//! then churns the identical expansion thousands of times until the global
//! per-query operation budget bails, leaving an opaque/deferred result instead
//! of the converged one.
//!
//! The active set now lives on [`EvaluationSession`](crate::evaluation::session::EvaluationSession).
//! This adapter keeps the existing fresh-evaluator call sites stable while
//! routing the state through the session owner. Re-entering a `TypeId` already
//! in flight is a cross-instance cycle: the caller skips the re-expansion and
//! treats the type as not-yet-resolved (exactly how the per-instance guard
//! treats a within-instance cycle), which lets the in-flight expansion — the one
//! holding the real work — converge and breaks the churn.

use crate::evaluation::result::EvaluationMemoResult;
use crate::evaluation::session::EvaluationSession;
use crate::types::TypeId;

/// Look up a memoized fresh-evaluator result for the current top-level query.
///
/// The key includes `no_unchecked_indexed_access` because evaluation results
/// depend on it (the per-checker `eval_cache` keys on the same flag): a memo
/// keyed on `TypeId` alone would return a result computed under the wrong mode.
pub(crate) fn query_memo_get(
    session: &EvaluationSession,
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
) -> Option<TypeId> {
    session.query_memo_get(type_id, no_unchecked_indexed_access)
}

/// Record a stable fresh-evaluator result for the current top-level query.
///
/// Callers must only store results that did not hit a recursion/budget limit
/// (`TypeEvaluator::recursion_limit_hit`), so an opaque cycle/budget bail is
/// never cached and reused as if it were the converged answer.
pub(crate) fn query_memo_put(
    session: &EvaluationSession,
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
    result: TypeId,
) {
    session.query_memo_put(type_id, no_unchecked_indexed_access, result);
}

/// Clear the per-query memo. Invoked when a fresh top-level evaluation query
/// begins so results never leak across queries, threads, or files.
pub(crate) fn reset_query_memo(session: &EvaluationSession) {
    session.reset_query_memo();
}

/// Run a fresh sub-evaluator for `type_id` with cross-instance cycle breaking
/// and per-query memoization — the shared scaffold for the two fresh-evaluator
/// boundaries (`evaluate_for_infer_match` and `SubtypeChecker::evaluate_type`).
///
/// Returns:
/// - `Some(result)` on a memo hit or a completed evaluation, and
/// - `None` when `type_id` is already being expanded by an ancestor fresh
///   evaluator in this session (a cross-instance cycle) — the caller must then
///   return `type_id` unchanged **without** caching it elsewhere, since the
///   in-flight ancestor owns the real result.
///
/// `compute` runs the fresh evaluation and returns an [`EvaluationMemoResult`]
/// whose stability decides whether the per-query memo may store it.
pub(crate) fn memoized_eval(
    session: &EvaluationSession,
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
    compute: impl FnOnce() -> EvaluationMemoResult,
) -> Option<TypeId> {
    memoized_eval_with_stability(session, type_id, no_unchecked_indexed_access, compute)
        .map(EvaluationMemoResult::into_type_id)
}

/// Like [`memoized_eval`], but also reports whether the result is *stable*:
/// converged without tripping any recursion/depth/budget limit.
///
/// A memo hit is always stable (only stable results are stored). A fresh
/// computation reports the `memoizable` flag from `compute`. Callers that
/// treat a collapsed result (e.g. `unknown`) as suspicious can use the flag to
/// distinguish a genuinely evaluated answer from a recursion-bail artifact.
pub(crate) fn memoized_eval_with_stability(
    session: &EvaluationSession,
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
    compute: impl FnOnce() -> EvaluationMemoResult,
) -> Option<EvaluationMemoResult> {
    if let Some(cached) = query_memo_get(session, type_id, no_unchecked_indexed_access) {
        return Some(EvaluationMemoResult::cached(cached));
    }
    let _cross = match CrossEvalExpansionGuard::enter(session, type_id) {
        CrossEvalExpansionState::Entered(guard) => guard,
        CrossEvalExpansionState::AlreadyActive => return None,
    };
    let memo_result = compute();
    if memo_result.is_stable_for_per_query_memo() {
        query_memo_put(
            session,
            type_id,
            no_unchecked_indexed_access,
            memo_result.type_id(),
        );
    }
    Some(memo_result)
}

/// RAII membership guard for cross-evaluator expansion of a `TypeId`.
///
/// [`enter`](Self::enter) returns `None` when `type_id` is already being
/// expanded by an ancestor fresh evaluator on this thread (a cross-instance
/// cycle); the caller must then skip the expansion and return the type
/// unchanged. Otherwise it records membership and clears it on drop, so the set
/// is restored even if evaluation unwinds via panic.
#[must_use]
pub(crate) struct CrossEvalExpansionGuard<'a> {
    session: &'a EvaluationSession,
    type_id: TypeId,
}

/// Result of entering the cross-evaluator expansion active set.
///
/// `Entered` owns the RAII membership guard for this expansion. `AlreadyActive`
/// names the cross-instance cycle case that callers collapse to the existing
/// "skip expansion and return the input unchanged" behavior.
#[must_use]
pub(crate) enum CrossEvalExpansionState<'a> {
    Entered(CrossEvalExpansionGuard<'a>),
    AlreadyActive,
}

impl CrossEvalExpansionGuard<'_> {
    pub(crate) fn enter<'a>(
        session: &'a EvaluationSession,
        type_id: TypeId,
    ) -> CrossEvalExpansionState<'a> {
        if session.enter_cross_eval_type(type_id) {
            CrossEvalExpansionState::Entered(CrossEvalExpansionGuard { session, type_id })
        } else {
            CrossEvalExpansionState::AlreadyActive
        }
    }
}

impl Drop for CrossEvalExpansionGuard<'_> {
    fn drop(&mut self) {
        self.session.leave_cross_eval_type(self.type_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::result::{EvaluationRequestStability, EvaluationResult};

    #[test]
    fn memo_keys_on_no_unchecked_indexed_access() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(7);
        query_memo_put(&session, t, false, TypeId(70));
        query_memo_put(&session, t, true, TypeId(71));
        assert_eq!(query_memo_get(&session, t, false), Some(TypeId(70)));
        assert_eq!(query_memo_get(&session, t, true), Some(TypeId(71)));
        reset_query_memo(&session);
        assert_eq!(query_memo_get(&session, t, false), None);
    }

    #[test]
    fn non_stable_fresh_result_is_returned_but_not_memoized() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(8);

        let first = memoized_eval(&session, t, false, || {
            EvaluationMemoResult::unstable_complete(TypeId(80))
        });

        assert_eq!(first, Some(TypeId(80)));
        assert_eq!(query_memo_get(&session, t, false), None);

        let second = memoized_eval(&session, t, false, || {
            EvaluationMemoResult::cached(TypeId(81))
        });

        assert_eq!(second, Some(TypeId(81)));
        assert_eq!(query_memo_get(&session, t, false), Some(TypeId(81)));
        reset_query_memo(&session);
    }

    #[test]
    fn unresolved_def_fresh_result_is_returned_and_memoized_within_query() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(9);
        let mut calls = 0;

        let first = memoized_eval(&session, t, false, || {
            calls += 1;
            EvaluationMemoResult::for_depth_agnostic_memo(
                EvaluationResult::complete(TypeId(90)),
                EvaluationRequestStability::UnresolvedDef,
            )
        });

        assert_eq!(first, Some(TypeId(90)));
        assert_eq!(query_memo_get(&session, t, false), Some(TypeId(90)));

        let second = memoized_eval(&session, t, false, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(91))
        });

        assert_eq!(second, Some(TypeId(90)));
        assert_eq!(calls, 1);
        assert_eq!(query_memo_get(&session, t, false), Some(TypeId(90)));
        reset_query_memo(&session);
    }

    #[test]
    fn reentry_of_active_type_is_rejected() {
        let session = EvaluationSession::new();
        let t = TypeId(4242);
        let CrossEvalExpansionState::Entered(outer) = CrossEvalExpansionGuard::enter(&session, t)
        else {
            panic!("first entry succeeds");
        };
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, t),
                CrossEvalExpansionState::AlreadyActive
            ),
            "re-entering an in-flight TypeId must be rejected"
        );
        drop(outer);
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, t),
                CrossEvalExpansionState::Entered(_)
            ),
            "once the in-flight guard drops, the TypeId is enterable again"
        );
    }

    #[test]
    fn distinct_types_are_independent() {
        let session = EvaluationSession::new();
        let CrossEvalExpansionState::Entered(a) =
            CrossEvalExpansionGuard::enter(&session, TypeId(1))
        else {
            panic!("a enters");
        };
        let CrossEvalExpansionState::Entered(b) =
            CrossEvalExpansionGuard::enter(&session, TypeId(2))
        else {
            panic!("b enters independently");
        };
        drop(a);
        drop(b);
    }
}
