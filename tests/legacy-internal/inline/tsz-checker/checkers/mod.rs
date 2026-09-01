//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7ba273063b1ff1cfdeb77dc51e503e8443a39c29efd4b18cef7de1e68652238a 276 stack_overflow_tripped_starts_false
    #[test]
    fn stack_overflow_tripped_starts_false() {
        reset();
        assert!(!stack_overflow_tripped());
    }
// TSZ_INLINE_TEST_END 7ba273063b1ff1cfdeb77dc51e503e8443a39c29efd4b18cef7de1e68652238a

// TSZ_INLINE_TEST_BEGIN e47bb533017753adeb17b3720581ce9ac31c8f3e3ae6853ca2d752babab71a0d 282 unmeasurable_headroom_never_trips
    #[test]
    fn unmeasurable_headroom_never_trips() {
        // `wasm32` reports `None`; that must not count as critically low, or the
        // checker bails to `TypeId::ERROR` mid-recursion on every guarded entry
        // point (issue #13815 family).
        assert!(!measured_headroom_below(None, 1024 * 1024));
        assert!(!measured_headroom_below(None, usize::MAX));
    }
// TSZ_INLINE_TEST_END e47bb533017753adeb17b3720581ce9ac31c8f3e3ae6853ca2d752babab71a0d

// TSZ_INLINE_TEST_BEGIN 31b3dae277124af2e98174e4e060012e9a40685f374c0a7d723e988997bdee8b 291 measured_headroom_compares_against_threshold
    #[test]
    fn measured_headroom_compares_against_threshold() {
        assert!(measured_headroom_below(Some(256 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(4 * 1024 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(1024 * 1024), 1024 * 1024));
    }
// TSZ_INLINE_TEST_END 31b3dae277124af2e98174e4e060012e9a40685f374c0a7d723e988997bdee8b

// TSZ_INLINE_TEST_BEGIN bbc3bf7f90ac4dd3a5d65803263445f74e971b6b43276de3a00b6ed35174155f 298 trip_stack_overflow_flips_tripped_flag
    #[test]
    fn trip_stack_overflow_flips_tripped_flag() {
        reset();
        assert!(!stack_overflow_tripped());
        trip_stack_overflow();
        assert!(stack_overflow_tripped());
        reset();
    }
// TSZ_INLINE_TEST_END bbc3bf7f90ac4dd3a5d65803263445f74e971b6b43276de3a00b6ed35174155f

// TSZ_INLINE_TEST_BEGIN a7d4632389e8d8855230641eb945d683b4f2bfcbac3441355da560f3d164d235 307 reset_stack_overflow_flag_clears_tripped_bit_only
    #[test]
    fn reset_stack_overflow_flag_clears_tripped_bit_only() {
        reset();
        // Increment the probe counter a few times.
        for _ in 0..10 {
            should_probe_stack();
        }
        let counter_before_reset = STACK_STATE.get() & 0xFF;
        assert_ne!(counter_before_reset, 0, "counter should have advanced");

        trip_stack_overflow();
        assert!(stack_overflow_tripped());

        reset_stack_overflow_flag();
        // Tripped bit cleared but counter preserved (bit 15 != counter).
        assert!(!stack_overflow_tripped());
        assert_eq!(
            STACK_STATE.get() & 0xFF,
            counter_before_reset,
            "reset must not clear the probe counter"
        );
        reset();
    }
// TSZ_INLINE_TEST_END a7d4632389e8d8855230641eb945d683b4f2bfcbac3441355da560f3d164d235

// TSZ_INLINE_TEST_BEGIN 98f37325a95bff3531bf2d8e5c48eeabf758f5ccc8e3696d6467fd45928ac7ec 331 should_probe_stack_returns_true_every_16th_call
    #[test]
    fn should_probe_stack_returns_true_every_16th_call() {
        reset();
        // The counter increments on every call; the helper returns true
        // when `counter & 0x0F == 0`. Starting from 0, the FIRST call
        // increments to 1, returns false. The 16th call increments the
        // counter to 16, which `& 0x0F == 0`, returns true.
        let mut hits = 0usize;
        for _ in 0..32 {
            if should_probe_stack() {
                hits += 1;
            }
        }
        // Out of 32 increments (1..=32), exactly 2 of those values
        // (16 and 32) have `(counter & 0x0F) == 0`.
        assert_eq!(
            hits, 2,
            "should_probe_stack should return true exactly 2 times in 32 calls"
        );
        reset();
    }
// TSZ_INLINE_TEST_END 98f37325a95bff3531bf2d8e5c48eeabf758f5ccc8e3696d6467fd45928ac7ec

// TSZ_INLINE_TEST_BEGIN 5019e9decc6a8a4b4ad8536990f54d5acfb2bd0473ae0be456357d8e47054840 353 should_probe_stack_first_call_is_false
    #[test]
    fn should_probe_stack_first_call_is_false() {
        reset();
        // First call: counter goes 0 → 1. `1 & 0x0F == 1`, so returns false.
        assert!(!should_probe_stack());
        reset();
    }
// TSZ_INLINE_TEST_END 5019e9decc6a8a4b4ad8536990f54d5acfb2bd0473ae0be456357d8e47054840

// TSZ_INLINE_TEST_BEGIN 4352ad5483499d679d6402e9ee968460f7c9c4aa83e6867122debed07256cd40 361 should_probe_stack_preserves_tripped_bit
    #[test]
    fn should_probe_stack_preserves_tripped_bit() {
        reset();
        trip_stack_overflow();
        assert!(stack_overflow_tripped());
        // Run probe-stack many times — the tripped bit must survive.
        for _ in 0..20 {
            should_probe_stack();
        }
        assert!(
            stack_overflow_tripped(),
            "tripped bit must be preserved across should_probe_stack calls"
        );
        reset();
    }
// TSZ_INLINE_TEST_END 4352ad5483499d679d6402e9ee968460f7c9c4aa83e6867122debed07256cd40

// TSZ_INLINE_TEST_BEGIN ded3d6d4481979cb0215b2dabeb9ff1e7f83b65e940b84789819cb24a0ca49ca 377 counter_wraps_at_byte_boundary
    #[test]
    fn counter_wraps_at_byte_boundary() {
        reset();
        // The counter masks with 0xFF, so it wraps after 256 calls back to
        // 0 (whose `& 0x0F == 0` → returns true on call 256).
        for _ in 0..255 {
            should_probe_stack();
        }
        // After 255 calls, counter == 255. Call 256: 255 + 1 = 256, masked
        // to 0. `0 & 0x0F == 0` → true.
        assert!(should_probe_stack());
        reset();
    }
// TSZ_INLINE_TEST_END ded3d6d4481979cb0215b2dabeb9ff1e7f83b65e940b84789819cb24a0ca49ca

// TSZ_INLINE_TEST_BEGIN 4254e703ec8a94fd3b034c674edeaffbc036e5bbcad6c6a6b9ad2a7a2f8843e5 391 with_stack_guard_runs_body_when_healthy
    #[test]
    fn with_stack_guard_runs_body_when_healthy() {
        reset();
        let mut ran = false;
        let out = with_stack_guard(-1, || {
            ran = true;
            42
        });
        assert!(ran, "body must run when the breaker is healthy");
        assert_eq!(out, 42, "with_stack_guard must return the body result");
        reset();
    }
// TSZ_INLINE_TEST_END 4254e703ec8a94fd3b034c674edeaffbc036e5bbcad6c6a6b9ad2a7a2f8843e5

// TSZ_INLINE_TEST_BEGIN 22355cf46ef28ac39cae19ff2b65612dc455e50249acc101c8bb57f076189901 404 with_stack_guard_bails_without_running_body_when_tripped
    #[test]
    fn with_stack_guard_bails_without_running_body_when_tripped() {
        reset();
        trip_stack_overflow();
        let mut ran = false;
        let out = with_stack_guard(-1, || {
            ran = true;
            42
        });
        assert!(
            !ran,
            "body must NOT run once the shared breaker has tripped — this is what \
             unwedges the cross-context heritage-merge recursion (#14111)"
        );
        assert_eq!(
            out, -1,
            "with_stack_guard must return on_exhausted when tripped"
        );
        reset();
    }
// TSZ_INLINE_TEST_END 22355cf46ef28ac39cae19ff2b65612dc455e50249acc101c8bb57f076189901

// TSZ_INLINE_TEST_BEGIN ad97a7692c45e9ae74749978a670ff33eaa3d9bce2025ee5523611d364e93e16 425 with_stack_guard_does_not_trip_on_probe_ticks_when_headroom_is_ample
    #[test]
    fn with_stack_guard_does_not_trip_on_probe_ticks_when_headroom_is_ample() {
        reset();
        // Drive enough calls to cross several probe ticks. A real test thread has
        // ample headroom, so none of the probes should trip the breaker.
        for _ in 0..64 {
            let out = with_stack_guard(0u32, || 7u32);
            assert_eq!(out, 7, "body must keep running while headroom is ample");
        }
        assert!(
            !stack_overflow_tripped(),
            "ample-headroom probes must not trip the breaker"
        );
        reset();
    }
// TSZ_INLINE_TEST_END ad97a7692c45e9ae74749978a670ff33eaa3d9bce2025ee5523611d364e93e16

// TSZ_INLINE_TEST_BEGIN 1c6870429dbe3de78fcc0bf334edc7124085253bfe72c49b9005a2b9556a5a42 441 clear_all_thread_local_state_zeros_stack_state
    #[test]
    fn clear_all_thread_local_state_zeros_stack_state() {
        // Trip the breaker and advance the counter, then clear.
        trip_stack_overflow();
        for _ in 0..5 {
            should_probe_stack();
        }
        assert!(stack_overflow_tripped());
        clear_all_thread_local_state();
        assert!(
            !stack_overflow_tripped(),
            "clear_all_thread_local_state must clear the tripped bit"
        );
        assert_eq!(
            STACK_STATE.get(),
            0,
            "clear_all_thread_local_state must zero the entire STACK_STATE"
        );
    }
// TSZ_INLINE_TEST_END 1c6870429dbe3de78fcc0bf334edc7124085253bfe72c49b9005a2b9556a5a42

// TSZ_INLINE_TEST_BEGIN a719df593babde0b17fc781cca526181f0f9a56708b7217084607b2e30e655f5 468 clear_all_thread_local_state_resets_cross_arena_and_alias_guards
    /// Regression test for issue #10880: batch mode reuses one worker process
    /// across project rows and relies on `clear_all_thread_local_state` to
    /// isolate them. Several cross-arena / type-alias recursion guards use
    /// manual (non-RAII) enter/leave or push/pop, so a row that bails out
    /// mid-delegation can leave them dirty. Before the fix these guards were
    /// not part of the reset, leaking state into the next row. This asserts
    /// every such guard is zero/empty after the canonical reset.
    #[test]
    fn clear_all_thread_local_state_resets_cross_arena_and_alias_guards() {
        use crate::{state_domain, types_domain};

        // Dirty every guard the way an aborted mid-delegation row would.
        state_domain::state::set_cross_arena_depth_for_test(3);
        state_domain::state::set_cross_arena_bailout_epoch_for_test(7);
        state_domain::type_analysis::dirty_cross_file_recursion_guards_for_test();
        types_domain::dirty_type_resolution_guards_for_test();
        super::promise_checker_object_normalization::dirty_awaited_eval_thread_local_state_for_test(
        );

        assert_ne!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "precondition: cross-arena depth dirtied"
        );
        assert!(
            !state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "precondition: cross-file recursion guards dirtied"
        );
        assert!(
            !types_domain::type_resolution_guards_clear_for_test(),
            "precondition: type-resolution guards dirtied"
        );
        assert!(
            !super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "precondition: awaited-eval thread-locals dirtied"
        );

        clear_all_thread_local_state();

        assert_eq!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "clear_all_thread_local_state must reset the cross-arena delegation depth"
        );
        assert_eq!(
            state_domain::state::cross_arena_bailout_epoch_for_test(),
            0,
            "clear_all_thread_local_state must reset the cross-arena bailout epoch"
        );
        assert!(
            state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "clear_all_thread_local_state must reset cross-file interface depth and alias stack"
        );
        assert!(
            types_domain::type_resolution_guards_clear_for_test(),
            "clear_all_thread_local_state must reset alias-resolution depth, stack, and scratch pool"
        );
        assert!(
            super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "clear_all_thread_local_state must reset the awaited-eval cycle guard and clamp epoch"
        );
    }
// TSZ_INLINE_TEST_END a719df593babde0b17fc781cca526181f0f9a56708b7217084607b2e30e655f5

// TSZ_INLINE_TEST_BEGIN 7868d8f37a5a9b397e927476f9c6e0c0abe384566d39d73d7696bccc01fcf51d 531 reset_per_file_resolution_guards_clears_cross_arena_and_alias_guards
    /// The fresh-checker path runs [`reset_per_file_resolution_guards`] at every
    /// *file* boundary (in `check_file_for_parallel` / `reset_for_next_file`) so
    /// a file that bails mid-walk cannot leak a dirty recursion/cycle guard into
    /// the next file scheduled onto the same shared worker thread (#13255 /
    /// #13368 schedule-sensitivity). This asserts the per-file reset clears the
    /// same cross-arena / alias / awaited guards the compilation-boundary reset
    /// does — they must not survive a file boundary on any thread.
    #[test]
    fn reset_per_file_resolution_guards_clears_cross_arena_and_alias_guards() {
        use crate::{state_domain, types_domain};

        // Dirty every guard the way an aborted mid-walk file would.
        state_domain::state::set_cross_arena_depth_for_test(3);
        state_domain::state::set_cross_arena_bailout_epoch_for_test(7);
        state_domain::type_analysis::dirty_cross_file_recursion_guards_for_test();
        types_domain::dirty_type_resolution_guards_for_test();
        super::promise_checker_object_normalization::dirty_awaited_eval_thread_local_state_for_test(
        );

        reset_per_file_resolution_guards();

        assert_eq!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "per-file reset must clear the cross-arena delegation depth"
        );
        assert_eq!(
            state_domain::state::cross_arena_bailout_epoch_for_test(),
            0,
            "per-file reset must clear the cross-arena bailout epoch"
        );
        assert!(
            state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "per-file reset must clear cross-file interface depth and alias stack"
        );
        assert!(
            types_domain::type_resolution_guards_clear_for_test(),
            "per-file reset must clear alias-resolution depth, stack, and scratch pool"
        );
        assert!(
            super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "per-file reset must clear the awaited-eval cycle guard and clamp epoch"
        );
    }
// TSZ_INLINE_TEST_END 7868d8f37a5a9b397e927476f9c6e0c0abe384566d39d73d7696bccc01fcf51d

// TSZ_INLINE_TEST_BEGIN c38b58607a50192e80f725b925992624a9fdb8d139a0a2b4ac03e081e8918b58 576 cross_arena_depth_cap_records_bailout_epoch
    /// `enter_cross_arena_delegation` must record a bailout (advance the
    /// bailout epoch) when it refuses delegation at the depth cap, and must
    /// NOT advance it on an allowed delegation. The epoch is what
    /// `delegate_cross_arena_symbol_resolution` / `get_type_of_symbol` compare
    /// before/after a resolution to decide whether a provisional sentinel was
    /// minted under the cap and must be kept out of the persistent caches
    /// (#13846). Name-agnostic: exercises the guard primitive directly.
    #[test]
    fn cross_arena_depth_cap_records_bailout_epoch() {
        use crate::CheckerState;
        use crate::state_domain::state;

        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);

        // Below the cap: delegation is allowed and does not record a bailout.
        let guard = CheckerState::<'_>::enter_cross_arena_delegation()
            .expect("delegation below the depth cap must be allowed");
        drop(guard);
        assert_eq!(
            state::cross_arena_bailout_epoch_for_test(),
            0,
            "an allowed delegation must not record a bailout"
        );

        // At the cap: delegation is refused and records a bailout so the
        // enclosing resolution refuses to persist its incomplete result.
        state::set_cross_arena_depth_for_test(5);
        let before = state::cross_arena_bailout_epoch_for_test();
        assert!(
            CheckerState::<'_>::enter_cross_arena_delegation().is_none(),
            "delegation at the depth cap must be refused"
        );
        assert_ne!(
            state::cross_arena_bailout_epoch_for_test(),
            before,
            "a depth-cap bailout must advance the bailout epoch"
        );

        // Leave the thread-locals clean for sibling tests on this worker.
        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);
    }
// TSZ_INLINE_TEST_END c38b58607a50192e80f725b925992624a9fdb8d139a0a2b4ac03e081e8918b58

// TSZ_INLINE_TEST_BEGIN 13f5467c8ba39700918307afc2598f8fb3ba56f9f949b5f78379dc5a24e1abe7 616 cross_arena_delegation_guard_restores_depth_on_drop
    /// The cross-arena depth guard is RAII-owned: early returns and unwinds drop
    /// the guard instead of relying on every caller to remember a matching
    /// manual leave.
    #[test]
    fn cross_arena_delegation_guard_restores_depth_on_drop() {
        use crate::CheckerState;
        use crate::state_domain::state;

        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);

        {
            let _outer = CheckerState::<'_>::enter_cross_arena_delegation()
                .expect("outer delegation should enter");
            assert_eq!(state::cross_arena_depth_for_test(), 1);
            {
                let _inner = CheckerState::<'_>::enter_cross_arena_delegation()
                    .expect("nested delegation should enter");
                assert_eq!(state::cross_arena_depth_for_test(), 2);
            }
            assert_eq!(state::cross_arena_depth_for_test(), 1);
        }

        assert_eq!(
            state::cross_arena_depth_for_test(),
            0,
            "dropping the guards must restore the shared depth"
        );
        assert_eq!(
            state::cross_arena_bailout_epoch_for_test(),
            0,
            "successful guard scopes must not record a bailout"
        );
    }
// TSZ_INLINE_TEST_END 13f5467c8ba39700918307afc2598f8fb3ba56f9f949b5f78379dc5a24e1abe7
