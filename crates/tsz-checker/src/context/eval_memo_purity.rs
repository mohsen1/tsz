//! Debug-only purity invariant for the stamp-keyed assignability memos
//! (issue #13980).
//!
//! [`AssignabilityEvalMemo`](super::caches::AssignabilityEvalMemo),
//! [`AwaitedAssignabilityEvalMemo`](super::caches::AwaitedAssignabilityEvalMemo)
//! and [`AssignabilityFailureMemo`](super::caches::AssignabilityFailureMemo) all
//! encode the same contract: for a fixed
//! [`AssignabilityEvalStamp`](super::caches::AssignabilityEvalStamp) — the two
//! type-environment generations plus the symbol-type cache versions — the
//! memoized value is a *pure function* of the key, so "a hit always returns
//! exactly what a fresh evaluation under the current environment would".
//!
//! Issue #13980 traced a class of bug where that contract is silently broken:
//! the eagerness with which `ensure_refs_resolved` / `ensure_relation_input_ready`
//! forces `Lazy(DefId)` refs is a **hidden input** that is not folded into the
//! stamp, yet it can change which `TypeId` an expression evaluates to
//! (index-access, `Promise` unwrap, generic-constraint elaboration, ...). The
//! sound direction is consumption-driven forcing; until that lands, the
//! resolution mode is "load-bearing for type identity" and any laziness tweak
//! in the #12101 campaign can change computed types in a way the diagnostic
//! conformance floor does not catch.
//!
//! This module is the debug-only invariant the issue asks for. It does not
//! change the resolution strategy; it makes the latent footgun observable. The
//! detection point is cheap and exact: a memo `insert` that overwrites an
//! existing entry **for the same stamp** with a **different** value is, by the
//! purity contract, impossible — so when it happens a hidden input leaked. The
//! displaced value is already returned by `HashMap::insert`, so the check adds
//! no extra lookup, and the whole module compiles out of release builds.
//!
//! Reporting is non-fatal by default (a `tracing::warn!` plus a process-wide
//! counter) so it never destabilises CI on a path that is only latently
//! unsound. Set `TSZ_EVAL_MEMO_PURITY_PANIC=1` to escalate a divergence to a
//! panic when actively hunting the leak.

use std::fmt::Debug;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Label identifying which memo a divergence was observed in, for the warning
/// payload. Kept as a `&'static str` so callers pass a constant with no
/// allocation on the hot path.
pub(crate) const ASSIGNABILITY_EVAL_MEMO: &str = "assignability_eval_memo";
pub(crate) const AWAITED_ASSIGNABILITY_EVAL_MEMO: &str = "awaited_assignability_eval_memo";
pub(crate) const ASSIGNABILITY_FAILURE_MEMO: &str = "assignability_failure_memo";

/// Process-wide count of detected purity violations. Observability only; never
/// gates behaviour. Relaxed ordering is sufficient — readers (tests, optional
/// diagnostics) only need eventual visibility, not synchronisation with the
/// evaluation that bumped it.
static DIVERGENCE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Pure predicate behind the invariant: an overwrite is a purity violation iff
/// an entry already existed for the key (same stamp, since the memo clears on
/// any stamp move) and the freshly computed value differs from it. A first
/// insert (`None`) or an identical re-insert is always fine.
///
/// Factored out of [`record_insert`] so the core rule is unit-testable without
/// touching the process-wide counter or the tracing subscriber.
pub(crate) fn is_divergent_overwrite<V: PartialEq>(previous: Option<&V>, result: &V) -> bool {
    matches!(previous, Some(prev) if prev != result)
}

/// Record a memo insert and report when it violates the same-stamp purity
/// contract. `previous` is the value `HashMap::insert` just displaced (so this
/// adds no extra lookup) and `result` borrows the value now stored. Returns
/// whether a divergence was detected so callers and tests can react without
/// reading the global counter.
pub(crate) fn record_insert<K: Debug, V: PartialEq + Debug>(
    memo: &'static str,
    key: K,
    previous: Option<V>,
    result: &V,
) -> bool {
    if !is_divergent_overwrite(previous.as_ref(), result) {
        return false;
    }
    DIVERGENCE_COUNT.fetch_add(1, Ordering::Relaxed);
    let previous = previous.expect("divergent overwrite implies a previous entry");
    tracing::warn!(
        memo,
        key = ?key,
        previous = ?previous,
        recomputed = ?result,
        "eval-memo purity violation: same-stamp re-evaluation produced a \
         different result — a resolution-mode -> type-identity leak (issue #13980)"
    );
    if purity_panic_enabled() {
        panic!(
            "eval-memo purity violation in {memo}: key {key:?} mapped to \
             {previous:?} then {result:?} under one stamp (issue #13980)"
        );
    }
    true
}

/// Number of purity violations observed so far this process. Test/diagnostic
/// surface only.
pub(crate) fn divergence_count() -> u64 {
    DIVERGENCE_COUNT.load(Ordering::Relaxed)
}

fn purity_panic_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TSZ_EVAL_MEMO_PURITY_PANIC")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::super::caches::CachedAssignabilityAnalysis;
    use super::*;
    use tsz_solver::TypeId;

    #[test]
    fn first_insert_is_not_a_violation() {
        assert!(!is_divergent_overwrite(None, &TypeId::STRING));
    }

    #[test]
    fn identical_reinsert_is_not_a_violation() {
        assert!(!is_divergent_overwrite(
            Some(&TypeId::STRING),
            &TypeId::STRING
        ));
    }

    #[test]
    fn differing_reinsert_under_same_stamp_is_a_violation() {
        assert!(is_divergent_overwrite(
            Some(&TypeId::STRING),
            &TypeId::NUMBER
        ));
    }

    #[test]
    fn record_insert_reports_only_on_divergence() {
        // First write: nothing displaced, no violation.
        assert!(!record_insert(
            ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            None,
            &TypeId::NUMBER
        ));
        // Stable replay of the same result: still fine.
        assert!(!record_insert(
            ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            Some(TypeId::NUMBER),
            &TypeId::NUMBER
        ));
        // A different result for the same key under the same stamp is the leak.
        let before = divergence_count();
        assert!(record_insert(
            AWAITED_ASSIGNABILITY_EVAL_MEMO,
            TypeId::STRING,
            Some(TypeId::NUMBER),
            &TypeId::BOOLEAN
        ));
        assert!(
            divergence_count() > before,
            "a detected divergence must advance the process-wide counter"
        );
    }

    #[test]
    fn record_insert_generalizes_to_struct_valued_memos() {
        // The failure memo stores a struct, not a `TypeId`; the same contract
        // and detector apply to a divergent `related` verdict for one key.
        let related = CachedAssignabilityAnalysis {
            related: true,
            depth_exceeded: false,
            iteration_exceeded: false,
            weak_union_violation: false,
            failure_reason: None,
        };
        let mut unrelated = related.clone();
        unrelated.related = false;
        let key = (TypeId::STRING, TypeId::NUMBER, 0u16, false);

        assert!(!record_insert(
            ASSIGNABILITY_FAILURE_MEMO,
            key,
            Some(related.clone()),
            &related
        ));
        assert!(record_insert(
            ASSIGNABILITY_FAILURE_MEMO,
            key,
            Some(related),
            &unrelated
        ));
    }
}
