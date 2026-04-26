//! Cycle-tracking set for alias resolution, backed by [`RecursionGuard`].
//!
//! Alias resolution (`resolve_alias_symbol`, `resolve_qualified_symbol_inner`,
//! etc.) walks chains of `SymbolId`s that may cycle.  The historical
//! implementation passed a `&mut Vec<SymbolId>` through the call graph and used
//! `contains` for cycle detection.  This wrapper preserves that API — callers
//! are unaffected — but delegates the underlying set to the solver's canonical
//! [`RecursionGuard<SymbolId>`], giving O(1) cycle checks plus the profile's
//! depth and iteration caps for free.
//!
//! Semantics match the legacy Vec: [`push`] is idempotent-per-chain (a cycle
//! detected by the preceding [`contains`] check skips the push) and is **not**
//! paired with a [`pop`].  The [`RecursionGuard`] would normally panic on drop
//! with unleaved entries in debug builds; [`Drop`] here calls
//! [`RecursionGuard::reset`] first to preserve the accumulate-until-drop
//! semantics that callers rely on.

use tsz_binder::SymbolId;
use tsz_solver::recursion::{RecursionGuard, RecursionProfile, RecursionResult};

/// Depth bound preserved from the pre-migration `Vec::len()` check.
const MAX_ALIAS_RESOLUTION_DEPTH: u32 = 128;

pub(crate) struct AliasCycleTracker {
    guard: RecursionGuard<SymbolId>,
}

impl AliasCycleTracker {
    pub(crate) fn new() -> Self {
        Self {
            guard: RecursionGuard::with_profile(RecursionProfile::Custom {
                max_depth: MAX_ALIAS_RESOLUTION_DEPTH,
                max_iterations: 100_000,
            }),
        }
    }

    #[inline]
    pub(crate) fn contains(&self, sym: &SymbolId) -> bool {
        self.guard.is_visiting(sym)
    }

    /// Record `sym` as visited.  Returns `true` if the enter succeeded, `false`
    /// if the depth/iteration cap was hit or the symbol was already tracked.
    /// Callers that previously ignored `Vec::push` may ignore the result;
    /// depth is already gated by [`Self::len`].
    #[inline]
    pub(crate) fn push(&mut self, sym: SymbolId) -> bool {
        matches!(self.guard.enter(sym), RecursionResult::Entered)
    }

    /// Remove `sym` from the visiting set, mirroring the old `Vec::pop` call
    /// sites that were paired with a preceding `push`.
    #[inline]
    pub(crate) fn pop(&mut self, sym: SymbolId) {
        self.guard.leave(sym);
    }

    #[inline]
    pub(crate) const fn len(&self) -> usize {
        self.guard.depth() as usize
    }

    /// Iterate over the symbols currently tracked as visited.  Order is
    /// unspecified.  Used by callers that previously iterated `&Vec<SymbolId>`
    /// to inspect every alias seen on the chain (e.g. type-only propagation).
    pub(crate) fn iter(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.guard.visiting_iter().copied()
    }
}

impl<'a> IntoIterator for &'a AliasCycleTracker {
    type Item = SymbolId;
    type IntoIter = Box<dyn Iterator<Item = SymbolId> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl Default for AliasCycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AliasCycleTracker {
    fn drop(&mut self) {
        // Preserve accumulate-until-drop semantics — callers don't pair every
        // `push` with a `pop`, so reset before `RecursionGuard`'s debug drop
        // check would fire on unleaved entries.
        self.guard.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: wrap a `u32` in `SymbolId`.  The numeric value is opaque to the
    /// tracker (only `Hash`/`Eq` are used), so any disjoint `u32` values
    /// produce disjoint logical symbols for these tests.
    fn sym(n: u32) -> SymbolId {
        SymbolId(n)
    }

    // ---------- construction --------------------------------------------------

    #[test]
    fn new_tracker_is_empty() {
        let t = AliasCycleTracker::new();
        assert_eq!(t.len(), 0);
        assert!(!t.contains(&sym(0)));
        assert!(!t.contains(&sym(1)));
    }

    #[test]
    fn default_tracker_is_empty_and_equivalent_to_new() {
        let t: AliasCycleTracker = Default::default();
        assert_eq!(t.len(), 0);
        assert!(!t.contains(&sym(42)));
    }

    // ---------- push / contains / len ----------------------------------------

    #[test]
    fn push_records_symbol_and_returns_true() {
        let mut t = AliasCycleTracker::new();
        let s = sym(7);
        assert!(t.push(s));
        assert!(t.contains(&s));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn push_distinct_symbols_each_returns_true_and_grows_len() {
        let mut t = AliasCycleTracker::new();
        assert!(t.push(sym(1)));
        assert!(t.push(sym(2)));
        assert!(t.push(sym(3)));
        assert_eq!(t.len(), 3);
        assert!(t.contains(&sym(1)));
        assert!(t.contains(&sym(2)));
        assert!(t.contains(&sym(3)));
        assert!(!t.contains(&sym(4)));
    }

    #[test]
    fn push_same_symbol_twice_returns_false_on_second_attempt() {
        // Mirrors the cycle-detection semantics: callers gate `push` on a
        // preceding `contains` check; even if they did not, the underlying
        // `RecursionGuard::enter` returns `Cycle` (not `Entered`).
        let mut t = AliasCycleTracker::new();
        assert!(t.push(sym(5)));
        assert!(!t.push(sym(5)));
        // The visiting set still contains exactly one entry (idempotent at the
        // logical level — no double counting).
        assert_eq!(t.len(), 1);
        assert!(t.contains(&sym(5)));
    }

    #[test]
    fn contains_is_false_for_unrecorded_symbol() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(10));
        assert!(!t.contains(&sym(11)));
        assert!(!t.contains(&sym(0)));
    }

    // ---------- pop ----------------------------------------------------------

    #[test]
    fn pop_removes_symbol_and_decreases_len() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(1));
        t.push(sym(2));
        assert_eq!(t.len(), 2);
        t.pop(sym(1));
        assert_eq!(t.len(), 1);
        assert!(!t.contains(&sym(1)));
        assert!(t.contains(&sym(2)));
    }

    #[test]
    fn push_after_pop_succeeds_again_for_same_symbol() {
        let mut t = AliasCycleTracker::new();
        assert!(t.push(sym(99)));
        t.pop(sym(99));
        // After leaving the cycle, the same symbol may be re-entered.
        assert!(t.push(sym(99)));
        assert!(t.contains(&sym(99)));
    }

    // ---------- iter ---------------------------------------------------------

    #[test]
    fn iter_yields_all_tracked_symbols() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(1));
        t.push(sym(2));
        t.push(sym(3));

        let mut collected: Vec<u32> = t.iter().map(|s| s.0).collect();
        collected.sort_unstable();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn iter_on_empty_tracker_yields_nothing() {
        let t = AliasCycleTracker::new();
        assert_eq!(t.iter().count(), 0);
    }

    #[test]
    fn iter_excludes_popped_symbols() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(10));
        t.push(sym(20));
        t.pop(sym(10));
        let collected: Vec<u32> = t.iter().map(|s| s.0).collect();
        assert_eq!(collected, vec![20]);
    }

    // ---------- IntoIterator for &AliasCycleTracker --------------------------

    #[test]
    fn into_iterator_borrowed_yields_all_tracked_symbols() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(4));
        t.push(sym(5));

        // Use the `for &s in &t` borrowed iteration form, which exercises
        // the `IntoIterator for &AliasCycleTracker` impl.
        let mut seen: Vec<u32> = Vec::new();
        for s in &t {
            seen.push(s.0);
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![4, 5]);
    }

    #[test]
    fn into_iterator_borrowed_does_not_consume_tracker() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(7));

        // Iterate twice: borrowing must leave the tracker intact.
        let count1 = (&t).into_iter().count();
        let count2 = (&t).into_iter().count();
        assert_eq!(count1, 1);
        assert_eq!(count2, 1);
        assert_eq!(t.len(), 1);
        assert!(t.contains(&sym(7)));
    }

    // ---------- drop semantics -----------------------------------------------

    #[test]
    fn drop_with_unleaved_entries_does_not_panic() {
        // The whole reason this wrapper exists is to preserve the legacy
        // accumulate-until-drop pattern: callers `push` without a paired
        // `pop`. `Drop` calls `guard.reset()` first, so the underlying
        // `RecursionGuard`'s debug-mode "unleaved entries" assertion never
        // fires. This test would panic in debug builds if that contract
        // ever broke.
        let mut t = AliasCycleTracker::new();
        t.push(sym(1));
        t.push(sym(2));
        t.push(sym(3));
        // No pop calls; drop here.
        drop(t);
    }

    #[test]
    fn drop_with_no_entries_does_not_panic() {
        let t = AliasCycleTracker::new();
        drop(t);
    }

    // ---------- depth cap ----------------------------------------------------

    #[test]
    fn push_past_max_alias_resolution_depth_returns_false() {
        let mut t = AliasCycleTracker::new();
        // Fill exactly `MAX_ALIAS_RESOLUTION_DEPTH` distinct entries — each
        // succeeds.
        for i in 0..MAX_ALIAS_RESOLUTION_DEPTH {
            assert!(
                t.push(sym(i)),
                "push of distinct symbol {i} below the depth cap should succeed",
            );
        }
        assert_eq!(t.len() as u32, MAX_ALIAS_RESOLUTION_DEPTH);

        // The (cap + 1)-th distinct entry must be rejected.
        let over = sym(MAX_ALIAS_RESOLUTION_DEPTH + 1_000);
        assert!(
            !t.push(over),
            "push past MAX_ALIAS_RESOLUTION_DEPTH must return false",
        );
        assert!(!t.contains(&over));
        // Depth is unchanged after the failed push.
        assert_eq!(t.len() as u32, MAX_ALIAS_RESOLUTION_DEPTH);
    }

    #[test]
    fn pop_below_cap_makes_room_for_new_pushes() {
        // Verifies that `pop` releases slots: after filling to the cap and
        // popping one, exactly one further distinct push must succeed.
        let mut t = AliasCycleTracker::new();
        for i in 0..MAX_ALIAS_RESOLUTION_DEPTH {
            assert!(t.push(sym(i)));
        }
        // Cap reached — cap+1 distinct push fails.
        assert!(!t.push(sym(9_999)));

        t.pop(sym(0));
        // One slot free — pushing one new distinct symbol succeeds.
        assert!(t.push(sym(9_999)));
        assert!(t.contains(&sym(9_999)));
    }

    // ---------- mixed push/contains/pop sequence -----------------------------

    #[test]
    fn nested_alias_chain_simulation() {
        // Mirrors how `resolve_alias_symbol` walks a chain of aliases:
        //   - check `contains` before each step
        //   - `push` the next symbol if not already on the chain
        //   - `pop` on unwind for the paired call sites.
        let mut t = AliasCycleTracker::new();
        let chain = [sym(100), sym(101), sym(102), sym(103)];

        for &s in &chain {
            assert!(!t.contains(&s));
            assert!(t.push(s));
        }
        assert_eq!(t.len(), chain.len());
        for &s in &chain {
            assert!(t.contains(&s));
        }

        // Unwind in LIFO order.
        for &s in chain.iter().rev() {
            t.pop(s);
        }
        assert_eq!(t.len(), 0);
        for &s in &chain {
            assert!(!t.contains(&s));
        }
    }

    #[test]
    fn cycle_detected_via_contains_then_push_returns_false() {
        // Reproduces the pattern documented in the module preamble:
        //   if visited.contains(&sym) { /* cycle */ }
        //   else { visited.push(sym); recurse(); }
        // Even if a misbehaving caller skipped the `contains` guard, `push`
        // itself returns false on the cycle, preserving the invariant.
        let mut t = AliasCycleTracker::new();
        t.push(sym(50));
        assert!(t.contains(&sym(50)));
        assert!(!t.push(sym(50)));
        assert_eq!(t.len(), 1);
    }
}
