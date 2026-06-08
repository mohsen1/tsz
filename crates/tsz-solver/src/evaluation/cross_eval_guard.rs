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
//! This thread-local set records the `TypeId`s currently being expanded by a
//! fresh evaluator anywhere on the thread. Re-entering one that is already in
//! flight is a cross-instance cycle: the caller skips the re-expansion and
//! treats the type as not-yet-resolved (exactly how the per-instance guard
//! treats a within-instance cycle), which lets the in-flight expansion — the one
//! holding the real work — converge and breaks the churn.

use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

thread_local! {
    static ACTIVE: RefCell<FxHashSet<TypeId>> = RefCell::new(FxHashSet::default());

    /// Per-top-level-query result memo shared across *all* `TypeEvaluator`
    /// instances on the thread (#11586).
    ///
    /// The two fresh-evaluator boundaries — infer-pattern expansion
    /// (`evaluate_for_infer_match`) and the subtype checker
    /// (`SubtypeChecker::evaluate_type`) — each construct a brand-new evaluator
    /// with an empty result cache, so a recursive conditional/`infer` application
    /// (`Unbox<Box<2>>`, `Awaited<Promise<2>>`, …) is re-evaluated from scratch
    /// tens of thousands of times as the recursion fans out, exhausting the
    /// per-query operation budget and bailing opaque instead of converging.
    ///
    /// This memo records each root `TypeId` those boundaries evaluate, so a
    /// repeated evaluation of the same type within one query is served from the
    /// memo instead of recomputed. It is **cleared at the start of every
    /// top-level query** (see [`reset_query_memo`], called when the per-query
    /// budget frame count transitions from zero), so a result never crosses query
    /// or file boundaries — strictly tighter than the per-file isolation that
    /// motivated keeping the application cache thread-local (issue #9507). Only
    /// stable, key-determined results are stored; recursion-bailed artifacts are
    /// not (see the call sites).
    static QUERY_MEMO: RefCell<FxHashMap<(TypeId, bool), TypeId>> =
        RefCell::new(FxHashMap::default());
}

/// Look up a memoized fresh-evaluator result for the current top-level query.
///
/// The key includes `no_unchecked_indexed_access` because evaluation results
/// depend on it (the per-checker `eval_cache` keys on the same flag): a memo
/// keyed on `TypeId` alone would return a result computed under the wrong mode.
pub(crate) fn query_memo_get(type_id: TypeId, no_unchecked_indexed_access: bool) -> Option<TypeId> {
    QUERY_MEMO.with(|memo| {
        memo.borrow()
            .get(&(type_id, no_unchecked_indexed_access))
            .copied()
    })
}

/// Record a stable fresh-evaluator result for the current top-level query.
///
/// Callers must only store results that did not hit a recursion/budget limit
/// (`TypeEvaluator::recursion_limit_hit`), so an opaque cycle/budget bail is
/// never cached and reused as if it were the converged answer.
pub(crate) fn query_memo_put(type_id: TypeId, no_unchecked_indexed_access: bool, result: TypeId) {
    QUERY_MEMO.with(|memo| {
        memo.borrow_mut()
            .insert((type_id, no_unchecked_indexed_access), result);
    });
}

/// Clear the per-query memo. Invoked when a fresh top-level evaluation query
/// begins so results never leak across queries, threads, or files.
pub(crate) fn reset_query_memo() {
    QUERY_MEMO.with(|memo| memo.borrow_mut().clear());
}

/// Run a fresh sub-evaluator for `type_id` with cross-instance cycle breaking
/// and per-query memoization — the shared scaffold for the two fresh-evaluator
/// boundaries (`evaluate_for_infer_match` and `SubtypeChecker::evaluate_type`).
///
/// Returns:
/// - `Some(result)` on a memo hit or a completed evaluation, and
/// - `None` when `type_id` is already being expanded by an ancestor fresh
///   evaluator on this thread (a cross-instance cycle) — the caller must then
///   return `type_id` unchanged **without** caching it elsewhere, since the
///   in-flight ancestor owns the real result.
///
/// `compute` runs the fresh evaluation and returns `(result, memoizable)`;
/// `memoizable` must be `false` for a recursion/budget-bailed run so a
/// stack-context artifact is never stored and reused as the converged answer.
pub(crate) fn memoized_eval(
    type_id: TypeId,
    no_unchecked_indexed_access: bool,
    compute: impl FnOnce() -> (TypeId, bool),
) -> Option<TypeId> {
    if let Some(cached) = query_memo_get(type_id, no_unchecked_indexed_access) {
        return Some(cached);
    }
    let _cross = CrossEvalExpansionGuard::enter(type_id)?;
    let (result, memoizable) = compute();
    if memoizable {
        query_memo_put(type_id, no_unchecked_indexed_access, result);
    }
    Some(result)
}

/// RAII membership guard for cross-evaluator expansion of a `TypeId`.
///
/// [`enter`](Self::enter) returns `None` when `type_id` is already being
/// expanded by an ancestor fresh evaluator on this thread (a cross-instance
/// cycle); the caller must then skip the expansion and return the type
/// unchanged. Otherwise it records membership and clears it on drop, so the set
/// is restored even if evaluation unwinds via panic.
#[must_use]
pub(crate) struct CrossEvalExpansionGuard(TypeId);

impl CrossEvalExpansionGuard {
    pub(crate) fn enter(type_id: TypeId) -> Option<Self> {
        ACTIVE.with(|set| {
            if set.borrow_mut().insert(type_id) {
                Some(Self(type_id))
            } else {
                None
            }
        })
    }
}

impl Drop for CrossEvalExpansionGuard {
    fn drop(&mut self) {
        ACTIVE.with(|set| {
            set.borrow_mut().remove(&self.0);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memo_keys_on_no_unchecked_indexed_access() {
        reset_query_memo();
        let t = TypeId(7);
        query_memo_put(t, false, TypeId(70));
        query_memo_put(t, true, TypeId(71));
        assert_eq!(query_memo_get(t, false), Some(TypeId(70)));
        assert_eq!(query_memo_get(t, true), Some(TypeId(71)));
        reset_query_memo();
        assert_eq!(query_memo_get(t, false), None);
    }

    #[test]
    fn reentry_of_active_type_is_rejected() {
        let t = TypeId(4242);
        let outer = CrossEvalExpansionGuard::enter(t).expect("first entry succeeds");
        assert!(
            CrossEvalExpansionGuard::enter(t).is_none(),
            "re-entering an in-flight TypeId must be rejected"
        );
        drop(outer);
        assert!(
            CrossEvalExpansionGuard::enter(t).is_some(),
            "once the in-flight guard drops, the TypeId is enterable again"
        );
    }

    #[test]
    fn distinct_types_are_independent() {
        let a = CrossEvalExpansionGuard::enter(TypeId(1)).expect("a enters");
        let b = CrossEvalExpansionGuard::enter(TypeId(2)).expect("b enters independently");
        drop(a);
        drop(b);
    }
}
