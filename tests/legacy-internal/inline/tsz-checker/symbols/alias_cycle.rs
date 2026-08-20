//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/symbols/alias_cycle.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ae6d0c52cd9179d81916deb78f3e2516b2e66ebd3fe2674a055f0373f2ba8182 108 new_tracker_is_empty
    #[test]
    fn new_tracker_is_empty() {
        let t = AliasCycleTracker::new();
        assert_eq!(t.len(), 0);
        assert!(!t.contains(&sym(0)));
        assert!(!t.contains(&sym(1)));
    }
// TSZ_INLINE_TEST_END ae6d0c52cd9179d81916deb78f3e2516b2e66ebd3fe2674a055f0373f2ba8182

// TSZ_INLINE_TEST_BEGIN 7127298de57a1945966303840d4434b62ca8248f2810e8e8a98f6b4592b4a387 116 default_tracker_is_empty_and_equivalent_to_new
    #[test]
    fn default_tracker_is_empty_and_equivalent_to_new() {
        let t: AliasCycleTracker = Default::default();
        assert_eq!(t.len(), 0);
        assert!(!t.contains(&sym(42)));
    }
// TSZ_INLINE_TEST_END 7127298de57a1945966303840d4434b62ca8248f2810e8e8a98f6b4592b4a387

// TSZ_INLINE_TEST_BEGIN a475cd0ea6bfdc8f0c4be39fb31a09c16b068a72bd110cf16c3b16cd2a360bb2 125 push_records_symbol_and_returns_true
    #[test]
    fn push_records_symbol_and_returns_true() {
        let mut t = AliasCycleTracker::new();
        let s = sym(7);
        assert!(t.push(s));
        assert!(t.contains(&s));
        assert_eq!(t.len(), 1);
    }
// TSZ_INLINE_TEST_END a475cd0ea6bfdc8f0c4be39fb31a09c16b068a72bd110cf16c3b16cd2a360bb2

// TSZ_INLINE_TEST_BEGIN c5ef3b790e3ca54486ad23f313002fcf1a824d79e8655e4196aac0dbaac2e593 134 push_distinct_symbols_each_returns_true_and_grows_len
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
// TSZ_INLINE_TEST_END c5ef3b790e3ca54486ad23f313002fcf1a824d79e8655e4196aac0dbaac2e593

// TSZ_INLINE_TEST_BEGIN b011e9458ad30db578ef5fbd3a0e02113898c5e97ddb84570b8c8d5be3df63ed 147 push_same_symbol_twice_returns_false_on_second_attempt
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
// TSZ_INLINE_TEST_END b011e9458ad30db578ef5fbd3a0e02113898c5e97ddb84570b8c8d5be3df63ed

// TSZ_INLINE_TEST_BEGIN 159f2e6a43ba7f9df2bea83481b8a45b9a1901eb69fa225230def61db92bcda4 161 contains_is_false_for_unrecorded_symbol
    #[test]
    fn contains_is_false_for_unrecorded_symbol() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(10));
        assert!(!t.contains(&sym(11)));
        assert!(!t.contains(&sym(0)));
    }
// TSZ_INLINE_TEST_END 159f2e6a43ba7f9df2bea83481b8a45b9a1901eb69fa225230def61db92bcda4

// TSZ_INLINE_TEST_BEGIN 960be1c197e5f0a9a73a3dc2ddc92e52fdad94a4e5c649d8d40f78e2498b126c 171 pop_removes_symbol_and_decreases_len
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
// TSZ_INLINE_TEST_END 960be1c197e5f0a9a73a3dc2ddc92e52fdad94a4e5c649d8d40f78e2498b126c

// TSZ_INLINE_TEST_BEGIN f2d9f81d62d3cbcb9e12d2cf669fb3e258312eb914fee3aef4a22a2e1b1f4d77 183 push_after_pop_succeeds_again_for_same_symbol
    #[test]
    fn push_after_pop_succeeds_again_for_same_symbol() {
        let mut t = AliasCycleTracker::new();
        assert!(t.push(sym(99)));
        t.pop(sym(99));
        // After leaving the cycle, the same symbol may be re-entered.
        assert!(t.push(sym(99)));
        assert!(t.contains(&sym(99)));
    }
// TSZ_INLINE_TEST_END f2d9f81d62d3cbcb9e12d2cf669fb3e258312eb914fee3aef4a22a2e1b1f4d77

// TSZ_INLINE_TEST_BEGIN 43b1e4e9c4e38631503e628140c65fbce74adfa1721616ea60c04c082ce1f7bc 195 iter_yields_all_tracked_symbols
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
// TSZ_INLINE_TEST_END 43b1e4e9c4e38631503e628140c65fbce74adfa1721616ea60c04c082ce1f7bc

// TSZ_INLINE_TEST_BEGIN 1215df82e7929a662eefd29fd6c11612930f9563ccfa04001263fb7b7d3c4f29 207 iter_on_empty_tracker_yields_nothing
    #[test]
    fn iter_on_empty_tracker_yields_nothing() {
        let t = AliasCycleTracker::new();
        assert_eq!(t.iter().count(), 0);
    }
// TSZ_INLINE_TEST_END 1215df82e7929a662eefd29fd6c11612930f9563ccfa04001263fb7b7d3c4f29

// TSZ_INLINE_TEST_BEGIN 4564d558e6e554b1981c530722dc3b130ba112a274f4662526f01ba5f72e2d5e 213 iter_excludes_popped_symbols
    #[test]
    fn iter_excludes_popped_symbols() {
        let mut t = AliasCycleTracker::new();
        t.push(sym(10));
        t.push(sym(20));
        t.pop(sym(10));
        let collected: Vec<u32> = t.iter().map(|s| s.0).collect();
        assert_eq!(collected, vec![20]);
    }
// TSZ_INLINE_TEST_END 4564d558e6e554b1981c530722dc3b130ba112a274f4662526f01ba5f72e2d5e

// TSZ_INLINE_TEST_BEGIN 418b03ed47b3af3f179b2e0667c91e88533ae6b09f067b434f670ea28d444460 225 into_iterator_borrowed_yields_all_tracked_symbols
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
// TSZ_INLINE_TEST_END 418b03ed47b3af3f179b2e0667c91e88533ae6b09f067b434f670ea28d444460

// TSZ_INLINE_TEST_BEGIN 0d0c429de7a62ec7718ff9aaf26c795ee12e6a0e3aad39a2f2f2a7bf9669d80d 241 into_iterator_borrowed_does_not_consume_tracker
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
// TSZ_INLINE_TEST_END 0d0c429de7a62ec7718ff9aaf26c795ee12e6a0e3aad39a2f2f2a7bf9669d80d

// TSZ_INLINE_TEST_BEGIN d9986c46b61f4bfa58de5e9ab11ac0ef251fec6bbe28d95ca2c09b1cad7a0d47 257 drop_with_unleaved_entries_does_not_panic
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
// TSZ_INLINE_TEST_END d9986c46b61f4bfa58de5e9ab11ac0ef251fec6bbe28d95ca2c09b1cad7a0d47

// TSZ_INLINE_TEST_BEGIN e462862d595904665322634c9b941a1bccf89f7b156816a78e08a2c1cf3d7ef1 273 drop_with_no_entries_does_not_panic
    #[test]
    fn drop_with_no_entries_does_not_panic() {
        let t = AliasCycleTracker::new();
        drop(t);
    }
// TSZ_INLINE_TEST_END e462862d595904665322634c9b941a1bccf89f7b156816a78e08a2c1cf3d7ef1

// TSZ_INLINE_TEST_BEGIN 1eccbb27c0d883ae2f04fc093f6fb9c3c025f42739c28d3eece1f490b386b46f 281 push_past_max_alias_resolution_depth_returns_false
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
// TSZ_INLINE_TEST_END 1eccbb27c0d883ae2f04fc093f6fb9c3c025f42739c28d3eece1f490b386b46f

// TSZ_INLINE_TEST_BEGIN 0ce0ade75735f077099da840530a896da4cd2bf2b9a759aaf2868055a782d6b0 305 pop_below_cap_makes_room_for_new_pushes
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
// TSZ_INLINE_TEST_END 0ce0ade75735f077099da840530a896da4cd2bf2b9a759aaf2868055a782d6b0

// TSZ_INLINE_TEST_BEGIN 25f0c0685a134eb814f9f5ee58f3b200c8b05ba8596b04719afb3c846e001d20 324 nested_alias_chain_simulation
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
// TSZ_INLINE_TEST_END 25f0c0685a134eb814f9f5ee58f3b200c8b05ba8596b04719afb3c846e001d20

// TSZ_INLINE_TEST_BEGIN 8fa8fff8c92ee4d917f1b78fd2de7648f0e64e432b7c08fb8f386e0f903bdf19 352 cycle_detected_via_contains_then_push_returns_false
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
// TSZ_INLINE_TEST_END 8fa8fff8c92ee4d917f1b78fd2de7648f0e64e432b7c08fb8f386e0f903bdf19
