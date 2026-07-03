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
//! routing the state through the session owner. Re-entering the same evaluation
//! request already in flight is a cross-instance cycle: the caller skips the
//! re-expansion and treats the type as not-yet-resolved (exactly how the
//! per-instance guard treats a within-instance cycle), which lets the in-flight
//! expansion — the one holding the real work — converge and breaks the churn.

use crate::evaluation::request::{EvaluationCacheKey, EvaluationRequest};
use crate::evaluation::result::EvaluationMemoResult;
use crate::evaluation::session::EvaluationSession;
use crate::types::TypeId;

/// Look up a memoized fresh-evaluator result for the current top-level query.
///
/// The key includes the index-access option pair and resolver generation
/// because evaluation results depend on both flags and on resolver-visible
/// lazy-body state. A memo keyed on `TypeId` alone would return a result
/// computed under the wrong mode or registration window.
pub(crate) fn query_memo_get(
    session: &EvaluationSession,
    key: EvaluationCacheKey,
) -> Option<TypeId> {
    session.query_memo_get(key)
}

/// Record a stable fresh-evaluator result for the current top-level query.
///
/// Callers must only store results that did not hit a recursion/budget limit
/// (`TypeEvaluator::recursion_limit_hit`), so an opaque cycle/budget bail is
/// never cached and reused as if it were the converged answer.
pub(crate) fn query_memo_put(session: &EvaluationSession, key: EvaluationCacheKey, result: TypeId) {
    session.query_memo_put(key, result);
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
/// - `None` when the request is already being expanded by an ancestor fresh
///   evaluator in this session (a cross-instance cycle) — the caller must then
///   return the input unchanged **without** caching it elsewhere, since the
///   in-flight ancestor owns the real result.
///
/// `compute` runs the fresh evaluation and returns an [`EvaluationMemoResult`]
/// whose stability decides whether the per-query memo may store it.
pub(crate) fn memoized_eval(
    session: &EvaluationSession,
    request: EvaluationRequest,
    compute: impl FnOnce() -> EvaluationMemoResult,
) -> Option<TypeId> {
    memoized_eval_with_stability(session, request, compute).map(EvaluationMemoResult::into_type_id)
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
    request: EvaluationRequest,
    compute: impl FnOnce() -> EvaluationMemoResult,
) -> Option<EvaluationMemoResult> {
    let key = request.cache_key();
    if let Some(cached) = query_memo_get(session, key) {
        return Some(EvaluationMemoResult::cached(cached));
    }
    let _cross = match CrossEvalExpansionGuard::enter(session, key) {
        CrossEvalExpansionState::Entered(guard) => guard,
        CrossEvalExpansionState::AlreadyActive => return None,
    };
    let memo_result = compute();
    if memo_result.is_stable_for_per_query_memo() {
        query_memo_put(session, key, memo_result.type_id());
    }
    Some(memo_result)
}

/// RAII membership guard for cross-evaluator expansion of an evaluation request.
///
/// [`enter`](Self::enter) returns `None` when `key` is already being
/// expanded by an ancestor fresh evaluator on this thread (a cross-instance
/// cycle); the caller must then skip the expansion and return the type
/// unchanged. Otherwise it records membership and clears it on drop, so the set
/// is restored even if evaluation unwinds via panic.
#[must_use]
pub(crate) struct CrossEvalExpansionGuard<'a> {
    session: &'a EvaluationSession,
    key: EvaluationCacheKey,
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
        key: EvaluationCacheKey,
    ) -> CrossEvalExpansionState<'a> {
        if session.enter_cross_eval_request(key) {
            CrossEvalExpansionState::Entered(CrossEvalExpansionGuard { session, key })
        } else {
            CrossEvalExpansionState::AlreadyActive
        }
    }
}

impl Drop for CrossEvalExpansionGuard<'_> {
    fn drop(&mut self) {
        self.session.leave_cross_eval_request(self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::result::{EvaluationRequestStability, EvaluationResult};

    #[test]
    fn memo_keys_on_index_access_options() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(7);
        let default_key = EvaluationCacheKey::new(t, false, false);
        let no_unchecked_key = EvaluationCacheKey::new(t, true, false);
        let exact_optional_key = EvaluationCacheKey::new(t, false, true);
        let both_key = EvaluationCacheKey::new(t, true, true);
        let resolver_key = default_key.with_resolver_generation(7);
        query_memo_put(&session, default_key, TypeId(70));
        query_memo_put(&session, no_unchecked_key, TypeId(71));
        query_memo_put(&session, exact_optional_key, TypeId(72));
        query_memo_put(&session, resolver_key, TypeId(73));
        assert_eq!(query_memo_get(&session, default_key), Some(TypeId(70)));
        assert_eq!(query_memo_get(&session, no_unchecked_key), Some(TypeId(71)));
        assert_eq!(
            query_memo_get(&session, exact_optional_key),
            Some(TypeId(72))
        );
        assert_eq!(query_memo_get(&session, resolver_key), Some(TypeId(73)));
        assert_eq!(query_memo_get(&session, both_key), None);
        assert_eq!(
            query_memo_get(&session, default_key.with_resolver_generation(8)),
            None
        );
        reset_query_memo(&session);
        assert_eq!(query_memo_get(&session, default_key), None);
        assert_eq!(query_memo_get(&session, resolver_key), None);
    }

    #[test]
    fn non_stable_fresh_result_is_returned_but_not_memoized() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(8);

        let request = EvaluationRequest::new(t);

        let first = memoized_eval(&session, request, || {
            EvaluationMemoResult::unstable_complete(TypeId(80))
        });

        assert_eq!(first, Some(TypeId(80)));
        assert_eq!(query_memo_get(&session, request.cache_key()), None);

        let second = memoized_eval(&session, request, || {
            EvaluationMemoResult::cached(TypeId(81))
        });

        assert_eq!(second, Some(TypeId(81)));
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(TypeId(81))
        );
        reset_query_memo(&session);
    }

    #[test]
    fn unresolved_def_fresh_result_is_returned_and_memoized_within_query() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(9);
        let request = EvaluationRequest::new(t);
        let mut calls = 0;

        let first = memoized_eval(&session, request, || {
            calls += 1;
            EvaluationMemoResult::for_depth_agnostic_memo(
                EvaluationResult::complete(TypeId(90)),
                EvaluationRequestStability::UnresolvedDef,
            )
        });

        assert_eq!(first, Some(TypeId(90)));
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(TypeId(90))
        );

        let second = memoized_eval(&session, request, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(91))
        });

        assert_eq!(second, Some(TypeId(90)));
        assert_eq!(calls, 1);
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(TypeId(90))
        );
        reset_query_memo(&session);
    }

    #[test]
    fn memoized_eval_partitions_by_resolver_generation() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let base = EvaluationRequest::new(TypeId(10));
        let gen_one = base.with_resolver_generation(1);
        let gen_two = base.with_resolver_generation(2);
        let mut calls = 0;

        let first = memoized_eval(&session, gen_one, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(100))
        });
        let second_same_generation = memoized_eval(&session, gen_one, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(101))
        });
        let third_new_generation = memoized_eval(&session, gen_two, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(200))
        });

        assert_eq!(first, Some(TypeId(100)));
        assert_eq!(second_same_generation, Some(TypeId(100)));
        assert_eq!(third_new_generation, Some(TypeId(200)));
        assert_eq!(calls, 2);
        assert_eq!(
            query_memo_get(&session, gen_one.cache_key()),
            Some(TypeId(100))
        );
        assert_eq!(
            query_memo_get(&session, gen_two.cache_key()),
            Some(TypeId(200))
        );
        reset_query_memo(&session);
    }

    #[test]
    fn reentry_of_active_type_is_rejected() {
        let session = EvaluationSession::new();
        let t = TypeId(4242);
        let key = EvaluationCacheKey::new(t, false, false);
        let CrossEvalExpansionState::Entered(outer) = CrossEvalExpansionGuard::enter(&session, key)
        else {
            panic!("first entry succeeds");
        };
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, key),
                CrossEvalExpansionState::AlreadyActive
            ),
            "re-entering an in-flight TypeId must be rejected"
        );
        drop(outer);
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, key),
                CrossEvalExpansionState::Entered(_)
            ),
            "once the in-flight guard drops, the TypeId is enterable again"
        );
    }

    #[test]
    fn active_set_partitions_by_full_request_key() {
        let session = EvaluationSession::new();
        let base = EvaluationCacheKey::new(TypeId(4243), false, false)
            .with_type_database_identity(1)
            .with_resolver_identity(10)
            .with_resolver_generation(1);
        let different_generation = base.with_resolver_generation(2);
        let different_resolver = base.with_resolver_identity(11);
        let different_arena = base.with_type_database_identity(2);

        let CrossEvalExpansionState::Entered(base_guard) =
            CrossEvalExpansionGuard::enter(&session, base)
        else {
            panic!("base request enters");
        };
        let CrossEvalExpansionState::Entered(generation_guard) =
            CrossEvalExpansionGuard::enter(&session, different_generation)
        else {
            panic!("same TypeId with a different generation enters independently");
        };
        let CrossEvalExpansionState::Entered(resolver_guard) =
            CrossEvalExpansionGuard::enter(&session, different_resolver)
        else {
            panic!("same TypeId with a different resolver enters independently");
        };
        let CrossEvalExpansionState::Entered(arena_guard) =
            CrossEvalExpansionGuard::enter(&session, different_arena)
        else {
            panic!("same TypeId with a different arena enters independently");
        };

        drop(base_guard);
        drop(generation_guard);
        drop(resolver_guard);
        drop(arena_guard);
    }

    #[test]
    fn distinct_types_are_independent() {
        let session = EvaluationSession::new();
        let CrossEvalExpansionState::Entered(a) = CrossEvalExpansionGuard::enter(
            &session,
            EvaluationCacheKey::new(TypeId(1), false, false),
        ) else {
            panic!("a enters");
        };
        let CrossEvalExpansionState::Entered(b) = CrossEvalExpansionGuard::enter(
            &session,
            EvaluationCacheKey::new(TypeId(2), false, false),
        ) else {
            panic!("b enters independently");
        };
        drop(a);
        drop(b);
    }
}
