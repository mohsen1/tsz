use super::*;

// ===================================================================
// RecursionProfile tests
// ===================================================================

#[test]
fn profile_subtype_check_limits() {
    let p = RecursionProfile::SubtypeCheck;
    assert_eq!(p.max_depth(), 100);
    assert_eq!(p.max_iterations(), 100_000);
}

#[test]
fn profile_type_evaluation_limits() {
    let p = RecursionProfile::TypeEvaluation;
    assert_eq!(p.max_depth(), 50);
    assert_eq!(p.max_iterations(), 100_000);
}

#[test]
fn profile_shallow_traversal_limits() {
    let p = RecursionProfile::ShallowTraversal;
    assert_eq!(p.max_depth(), 20);
    assert_eq!(p.max_iterations(), 100_000);
}

#[test]
fn profile_custom_limits() {
    let p = RecursionProfile::Custom {
        max_depth: 7,
        max_iterations: 42,
    };
    assert_eq!(p.max_depth(), 7);
    assert_eq!(p.max_iterations(), 42);
}

#[test]
fn with_profile_constructor() {
    let guard = RecursionGuard::<u32>::with_profile(RecursionProfile::SubtypeCheck);
    assert_eq!(guard.max_depth(), 100);
    assert_eq!(guard.max_iterations(), 100_000);
    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.iterations(), 0);
    assert!(!guard.is_exceeded());
    assert!(!guard.is_active());
}

// ===================================================================
// Core enter/leave tests
// ===================================================================

#[test]
fn basic_enter_leave() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.depth(), 1);
    assert_eq!(guard.visiting_count(), 1);
    assert!(guard.is_visiting(&1));
    assert!(guard.is_active());

    guard.leave(1);
    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.visiting_count(), 0);
    assert!(!guard.is_visiting(&1));
    assert!(!guard.is_active());
}

#[test]
fn enter_increments_iterations() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.iterations(), 0);

    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.iterations(), 1);

    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.iterations(), 2);

    guard.leave(2);
    guard.leave(1);
    // leave does not decrement iterations
    assert_eq!(guard.iterations(), 2);
}

#[test]
fn nested_different_keys() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);

    assert_eq!(guard.depth(), 3);
    assert_eq!(guard.visiting_count(), 3);
    assert!(guard.is_visiting(&1));
    assert!(guard.is_visiting(&2));
    assert!(guard.is_visiting(&3));

    guard.leave(3);
    guard.leave(2);
    guard.leave(1);
    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.visiting_count(), 0);
}

#[test]
fn reenter_after_leave() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    guard.leave(1);

    // Same key should be enterable again after leaving
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.depth(), 1);
    guard.leave(1);
}

// ===================================================================
// Cycle detection tests
// ===================================================================

#[test]
fn cycle_detected_on_same_key() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(1u32), RecursionResult::Cycle);

    // Cycle does NOT increment depth (entry was denied)
    assert_eq!(guard.depth(), 1);
    // But it DOES increment iterations (we tried)
    assert_eq!(guard.iterations(), 2);

    guard.leave(1);
}

#[test]
fn cycle_does_not_set_exceeded() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
    assert!(!guard.is_exceeded());
    guard.leave(1);
}

#[test]
fn cycle_detection_with_tuple_keys() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter((1u32, 2u32)), RecursionResult::Entered);
    assert_eq!(guard.enter((1u32, 3u32)), RecursionResult::Entered);

    // Same pair = cycle
    assert_eq!(guard.enter((1u32, 2u32)), RecursionResult::Cycle);
    // Different pair = ok
    assert_eq!(guard.enter((3u32, 4u32)), RecursionResult::Entered);

    guard.leave((3, 4));
    guard.leave((1, 3));
    guard.leave((1, 2));
    assert_eq!(guard.depth(), 0);
}

#[test]
fn cycle_direction_matters_for_tuples() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter((1u32, 2u32)), RecursionResult::Entered);
    // (2, 1) is NOT the same as (1, 2) — direction matters
    assert_eq!(guard.enter((2u32, 1u32)), RecursionResult::Entered);

    guard.leave((2, 1));
    guard.leave((1, 2));
}

// ===================================================================
// Depth limit tests
// ===================================================================

#[test]
fn depth_exceeded_at_max() {
    let mut guard = RecursionGuard::new(2, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    // depth = 2, max = 2, next enter should fail
    assert_eq!(guard.enter(3u32), RecursionResult::DepthExceeded);
    assert!(guard.is_exceeded());

    guard.leave(2);
    guard.leave(1);
}

#[test]
fn depth_exceeded_persists_after_leaving() {
    let mut guard = RecursionGuard::new(1, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::DepthExceeded);
    assert!(guard.is_exceeded());

    guard.leave(1);
    // exceeded flag stays true even after depth drops below limit
    assert!(guard.is_exceeded());
    assert_eq!(guard.depth(), 0);
}

#[test]
fn depth_zero_means_nothing_can_enter() {
    let mut guard = RecursionGuard::new(0, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::DepthExceeded);
    assert!(guard.is_exceeded());
}

#[test]
fn depth_one_allows_single_entry() {
    let mut guard = RecursionGuard::new(1, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::DepthExceeded);
    guard.leave(1);
}

// ===================================================================
// Iteration limit tests
// ===================================================================

#[test]
fn iteration_exceeded() {
    let mut guard = RecursionGuard::new(100, 3);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    guard.leave(1);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    guard.leave(2);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);
    guard.leave(3);
    // 4th attempt exceeds iteration limit
    assert_eq!(guard.enter(4u32), RecursionResult::IterationExceeded);
    assert!(guard.is_exceeded());
}

#[test]
fn iterations_count_all_attempts_including_denied() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.iterations(), 1);

    // Cycle also counts as an iteration
    assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
    assert_eq!(guard.iterations(), 2);

    guard.leave(1);
}

#[test]
fn iteration_zero_means_nothing_can_enter() {
    let mut guard = RecursionGuard::new(100, 0);
    assert_eq!(guard.enter(1u32), RecursionResult::IterationExceeded);
    assert!(guard.is_exceeded());
}

#[test]
fn iteration_one_allows_single_attempt() {
    let mut guard = RecursionGuard::new(100, 1);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    guard.leave(1);
    // Second attempt exceeds
    assert_eq!(guard.enter(2u32), RecursionResult::IterationExceeded);
}

#[test]
fn iteration_overflow_saturates() {
    // Use max_iterations < u32::MAX so that saturation actually exceeds the limit.
    let mut guard = RecursionGuard::new(u32::MAX, u32::MAX - 2);
    // Manually set iterations near saturation point
    guard.iterations = u32::MAX - 1;
    // iterations becomes u32::MAX via saturating_add, which is > max_iterations (u32::MAX - 2)
    assert_eq!(guard.enter(1u32), RecursionResult::IterationExceeded);
    assert_eq!(guard.iterations(), u32::MAX);
    assert!(guard.is_exceeded());
}

// ===================================================================
// Max visiting set size tests
// ===================================================================

#[test]
fn max_visiting_set_size_enforced() {
    let mut guard = RecursionGuard::new(1000, 100_000).with_max_visiting(3);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);
    // 4th entry: visiting set at capacity
    assert_eq!(guard.enter(4u32), RecursionResult::DepthExceeded);
    assert!(guard.is_exceeded());

    guard.leave(3);
    guard.leave(2);
    guard.leave(1);
}

#[test]
fn max_visiting_zero_blocks_all() {
    let mut guard = RecursionGuard::new(100, 100_000).with_max_visiting(0);
    assert_eq!(guard.enter(1u32), RecursionResult::DepthExceeded);
}

// ===================================================================
// Exceeded state tests
// ===================================================================

#[test]
fn mark_exceeded_manually() {
    let mut guard = RecursionGuard::<u32>::new(10, 100);
    assert!(!guard.is_exceeded());
    guard.mark_exceeded();
    assert!(guard.is_exceeded());
}

#[test]
fn exceeded_cleared_by_reset() {
    let mut guard = RecursionGuard::<u32>::new(10, 100);
    guard.mark_exceeded();
    assert!(guard.is_exceeded());
    guard.reset();
    assert!(!guard.is_exceeded());
}

// ===================================================================
// Reset tests
// ===================================================================

#[test]
fn reset_clears_all_state() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    guard.mark_exceeded();

    guard.reset();

    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.iterations(), 0);
    assert_eq!(guard.visiting_count(), 0);
    assert!(!guard.is_exceeded());
    assert!(!guard.is_active());
    assert!(!guard.is_visiting(&1));
    assert!(!guard.is_visiting(&2));

    // Should be enterable again
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    guard.leave(1);
}

#[test]
fn reset_preserves_limits() {
    let guard_before = RecursionGuard::<u32>::new(42, 999).with_max_visiting(7);
    let mut guard = RecursionGuard::<u32>::new(42, 999).with_max_visiting(7);
    guard.reset();
    assert_eq!(guard.max_depth(), guard_before.max_depth());
    assert_eq!(guard.max_iterations(), guard_before.max_iterations());
}

// ===================================================================
// Scope (closure-based RAII) tests
// ===================================================================

#[test]
fn scope_success() {
    let mut guard = RecursionGuard::new(10, 100);
    let result = guard.scope(1u32, || 42);
    assert_eq!(result, Ok(42));
    // After scope, key should be left
    assert!(!guard.is_visiting(&1));
    assert_eq!(guard.depth(), 0);
}

#[test]
fn scope_cycle() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);

    let result = guard.scope(1u32, || 42);
    assert_eq!(result, Err(RecursionResult::Cycle));

    guard.leave(1);
}

#[test]
fn scope_depth_exceeded() {
    let mut guard = RecursionGuard::new(1, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);

    let result = guard.scope(2u32, || 42);
    assert_eq!(result, Err(RecursionResult::DepthExceeded));

    guard.leave(1);
}

#[test]
fn scope_nested() {
    let mut guard = RecursionGuard::new(10, 100);
    let outer = guard.scope(1u32, || {
        // Can't nest scope calls because &mut is held — but we can
        // verify the function was called
        100
    });
    assert_eq!(outer, Ok(100));

    // Guard is fully unwound
    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.visiting_count(), 0);
}

// ===================================================================
// RecursionResult helper tests
// ===================================================================

#[test]
fn result_helpers() {
    assert!(RecursionResult::Entered.is_entered());
    assert!(!RecursionResult::Entered.is_cycle());
    assert!(!RecursionResult::Entered.is_exceeded());
    assert!(!RecursionResult::Entered.is_denied());

    assert!(!RecursionResult::Cycle.is_entered());
    assert!(RecursionResult::Cycle.is_cycle());
    assert!(!RecursionResult::Cycle.is_exceeded());
    assert!(RecursionResult::Cycle.is_denied());

    assert!(!RecursionResult::DepthExceeded.is_entered());
    assert!(!RecursionResult::DepthExceeded.is_cycle());
    assert!(RecursionResult::DepthExceeded.is_exceeded());
    assert!(RecursionResult::DepthExceeded.is_denied());

    assert!(!RecursionResult::IterationExceeded.is_entered());
    assert!(!RecursionResult::IterationExceeded.is_cycle());
    assert!(RecursionResult::IterationExceeded.is_exceeded());
    assert!(RecursionResult::IterationExceeded.is_denied());
}

// ===================================================================
// Priority / ordering tests
// ===================================================================

#[test]
fn iteration_checked_before_depth() {
    // If both iteration and depth would fail, iteration wins
    // (because iteration is checked first in enter())
    let mut guard = RecursionGuard::new(0, 0);
    let result = guard.enter(1u32);
    assert_eq!(result, RecursionResult::IterationExceeded);
}

#[test]
fn depth_checked_before_cycle() {
    // If both depth and cycle would fail, depth wins
    let mut guard = RecursionGuard::new(1, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    // Now depth=1, max=1. Key 1 is also visiting.
    // Entering key 1 again: depth check fires first
    assert_eq!(guard.enter(1u32), RecursionResult::DepthExceeded);
    guard.leave(1);
}

#[test]
fn cycle_checked_before_visiting_set_size() {
    // If both cycle and visiting-set-full would fail, cycle wins
    let mut guard = RecursionGuard::new(100, 100_000).with_max_visiting(1);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    // visiting set is full, and key 1 is already there
    // cycle check fires first because contains() is checked before len()
    assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
    guard.leave(1);
}

// ===================================================================
// Stress / boundary tests
// ===================================================================

#[test]
fn many_enter_leave_cycles() {
    let mut guard = RecursionGuard::new(10, 100_000);
    for i in 0u32..10_000 {
        assert_eq!(guard.enter(i), RecursionResult::Entered);
        guard.leave(i);
    }
    assert_eq!(guard.depth(), 0);
    assert_eq!(guard.visiting_count(), 0);
    assert_eq!(guard.iterations(), 10_000);
}

#[test]
fn max_depth_exact_boundary() {
    let mut guard = RecursionGuard::new(3, 100);
    // Enter exactly max_depth times
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);
    assert_eq!(guard.depth(), 3);
    // Next should fail
    assert_eq!(guard.enter(4u32), RecursionResult::DepthExceeded);

    guard.leave(3);
    guard.leave(2);
    guard.leave(1);
}

#[test]
fn leave_out_of_order() {
    // Leave in different order than enter — should work fine
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);

    // Leave in reverse order
    guard.leave(1);
    assert!(guard.is_visiting(&2));
    assert!(guard.is_visiting(&3));
    assert!(!guard.is_visiting(&1));
    assert_eq!(guard.depth(), 2);

    guard.leave(3);
    guard.leave(2);
    assert_eq!(guard.depth(), 0);
}

// ===================================================================
// is_visiting tests
// ===================================================================

#[test]
fn is_visiting_returns_false_for_unknown_key() {
    let guard = RecursionGuard::<u32>::new(10, 100);
    assert!(!guard.is_visiting(&999));
}

#[test]
fn is_visiting_tracks_active_keys_only() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);

    assert!(guard.is_visiting(&1));
    assert!(guard.is_visiting(&2));
    assert!(!guard.is_visiting(&3));

    guard.leave(1);
    assert!(!guard.is_visiting(&1));
    assert!(guard.is_visiting(&2));

    guard.leave(2);
}

// ===================================================================
// Complex key type tests
// ===================================================================

#[test]
fn bool_polarity_keys() {
    // Used by VarianceVisitor with (TypeId, bool) keys
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter((1u32, true)), RecursionResult::Entered);
    // Same type, different polarity = different key
    assert_eq!(guard.enter((1u32, false)), RecursionResult::Entered);
    // Same type and polarity = cycle
    assert_eq!(guard.enter((1u32, true)), RecursionResult::Cycle);

    guard.leave((1, false));
    guard.leave((1, true));
}

#[test]
fn three_element_tuple_keys() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter((1u32, 2u32, 3u32)), RecursionResult::Entered);
    assert_eq!(guard.enter((1u32, 2u32, 3u32)), RecursionResult::Cycle);
    assert_eq!(guard.enter((1u32, 2u32, 4u32)), RecursionResult::Entered);

    guard.leave((1, 2, 4));
    guard.leave((1, 2, 3));
}

// ===================================================================
// Debug assertion tests (only run in debug mode)
// ===================================================================

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "not in the visiting set")]
fn debug_leave_without_enter_panics() {
    let mut guard = RecursionGuard::new(10, 100);
    guard.leave(1u32); // No matching enter — should panic in debug
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "not in the visiting set")]
fn debug_double_leave_panics() {
    let mut guard = RecursionGuard::new(10, 100);
    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    guard.leave(1);
    guard.leave(1); // Second leave — should panic in debug
}

// ===================================================================
// Interaction between multiple limit types
// ===================================================================

#[test]
fn recovery_after_depth_exceeded() {
    let mut guard = RecursionGuard::new(2, 100);

    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::DepthExceeded);

    // Leave to reduce depth
    guard.leave(2);
    assert_eq!(guard.depth(), 1);

    // Even though depth is below limit, exceeded flag prevents naive
    // "retry" strategies from re-entering. Callers check is_exceeded()
    // independently. The guard itself still allows entry after depth drops:
    assert_eq!(guard.enter(4u32), RecursionResult::Entered);
    guard.leave(4);
    guard.leave(1);
}

#[test]
fn cycle_after_depth_recovery() {
    let mut guard = RecursionGuard::new(2, 100);

    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    guard.leave(2);

    // Re-enter key 1 (which is still visiting) = cycle, not depth
    assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
    guard.leave(1);
}

#[test]
fn interleaved_cycles_and_depth() {
    let mut guard = RecursionGuard::new(3, 100);

    assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    assert_eq!(guard.enter(2u32), RecursionResult::Entered);
    assert_eq!(guard.enter(3u32), RecursionResult::Entered);
    // Depth exhausted
    assert_eq!(guard.enter(4u32), RecursionResult::DepthExceeded);
    // But cycle detection still works at this depth
    assert_eq!(guard.enter(2u32), RecursionResult::DepthExceeded);
    // (depth check fires before cycle check)

    guard.leave(3);
    // Now depth=2, can try again
    assert_eq!(guard.enter(2u32), RecursionResult::Cycle);
    assert_eq!(guard.enter(5u32), RecursionResult::Entered);

    guard.leave(5);
    guard.leave(2);
    guard.leave(1);
}

// ===================================================================
// with_profile integration
// ===================================================================

#[test]
fn all_profiles_have_valid_limits() {
    let profiles = [
        RecursionProfile::SubtypeCheck,
        RecursionProfile::TypeEvaluation,
        RecursionProfile::TypeApplication,
        RecursionProfile::PropertyAccess,
        RecursionProfile::Variance,
        RecursionProfile::ShapeExtraction,
        RecursionProfile::ShallowTraversal,
        RecursionProfile::ConstAssertion,
        RecursionProfile::ExpressionCheck,
        RecursionProfile::TypeNodeCheck,
        RecursionProfile::CallResolution,
        RecursionProfile::CheckerRecursion,
    ];
    for profile in profiles {
        assert!(profile.max_depth() > 0, "{profile:?} has zero max_depth");
        assert!(
            profile.max_iterations() > 0,
            "{profile:?} has zero max_iterations"
        );
        assert!(
            profile.max_iterations() >= profile.max_depth(),
            "{profile:?} has max_iterations < max_depth"
        );

        // Verify both guard types can be constructed
        let guard = RecursionGuard::<u32>::with_profile(profile);
        assert_eq!(guard.max_depth(), profile.max_depth());
        assert_eq!(guard.max_iterations(), profile.max_iterations());

        let counter = DepthCounter::with_profile(profile);
        assert_eq!(counter.max_depth(), profile.max_depth());
    }
}

// ===================================================================
// DepthCounter tests
// ===================================================================

#[test]
fn dc_basic_enter_leave() {
    let mut dc = DepthCounter::new(10);
    assert_eq!(dc.depth(), 0);
    assert!(dc.enter());
    assert_eq!(dc.depth(), 1);
    dc.leave();
    assert_eq!(dc.depth(), 0);
}

#[test]
fn dc_with_profile() {
    let dc = DepthCounter::with_profile(RecursionProfile::ExpressionCheck);
    assert_eq!(dc.max_depth(), 500);
    assert_eq!(dc.depth(), 0);
    assert!(!dc.is_exceeded());
}

#[test]
fn dc_depth_exceeded_at_max() {
    let mut dc = DepthCounter::new(2);
    assert!(dc.enter());
    assert!(dc.enter());
    // depth = 2, max = 2, should fail
    assert!(!dc.enter());
    assert!(dc.is_exceeded());
    dc.leave();
    dc.leave();
}

#[test]
fn dc_exceeded_persists_after_leaving() {
    let mut dc = DepthCounter::new(1);
    assert!(dc.enter());
    assert!(!dc.enter()); // exceeded
    assert!(dc.is_exceeded());
    dc.leave();
    // Sticky flag
    assert!(dc.is_exceeded());
    assert_eq!(dc.depth(), 0);
}

#[test]
fn dc_zero_max_depth() {
    let mut dc = DepthCounter::new(0);
    assert!(!dc.enter());
    assert!(dc.is_exceeded());
}

#[test]
fn dc_one_max_depth() {
    let mut dc = DepthCounter::new(1);
    assert!(dc.enter());
    assert!(!dc.enter());
    dc.leave();
}

#[test]
fn dc_nested_enter_leave() {
    let mut dc = DepthCounter::new(10);
    assert!(dc.enter());
    assert!(dc.enter());
    assert!(dc.enter());
    assert_eq!(dc.depth(), 3);
    dc.leave();
    dc.leave();
    dc.leave();
    assert_eq!(dc.depth(), 0);
}

#[test]
fn dc_mark_exceeded() {
    let mut dc = DepthCounter::new(10);
    assert!(!dc.is_exceeded());
    dc.mark_exceeded();
    assert!(dc.is_exceeded());
}

#[test]
fn dc_reset() {
    let mut dc = DepthCounter::new(10);
    assert!(dc.enter());
    assert!(dc.enter());
    dc.mark_exceeded();

    dc.reset();

    assert_eq!(dc.depth(), 0);
    assert!(!dc.is_exceeded());
    // Can enter again
    assert!(dc.enter());
    dc.leave();
}

#[test]
fn dc_reset_preserves_max_depth() {
    let mut dc = DepthCounter::new(42);
    dc.reset();
    assert_eq!(dc.max_depth(), 42);
}

#[test]
fn dc_many_enter_leave_cycles() {
    let mut dc = DepthCounter::new(5);
    for _ in 0..1000 {
        assert!(dc.enter());
        dc.leave();
    }
    assert_eq!(dc.depth(), 0);
}

#[test]
fn dc_exact_boundary() {
    let mut dc = DepthCounter::new(3);
    assert!(dc.enter()); // 1
    assert!(dc.enter()); // 2
    assert!(dc.enter()); // 3
    assert!(!dc.enter()); // exceeded
    dc.leave();
    dc.leave();
    dc.leave();
}

#[test]
fn dc_recovery_after_exceeded() {
    let mut dc = DepthCounter::new(2);
    assert!(dc.enter());
    assert!(dc.enter());
    assert!(!dc.enter()); // exceeded
    dc.leave();
    // Depth dropped, can enter again
    assert!(dc.enter());
    dc.leave();
    dc.leave();
}

#[test]
fn dc_with_initial_depth() {
    let mut dc = DepthCounter::with_initial_depth(10, 5);
    assert_eq!(dc.depth(), 5);
    assert_eq!(dc.max_depth(), 10);

    // Can enter 5 more times (10 - 5 = 5 remaining)
    for _ in 0..5 {
        assert!(dc.enter());
    }
    assert_eq!(dc.depth(), 10);
    assert!(!dc.enter()); // exceeded

    // Leave back to base
    for _ in 0..5 {
        dc.leave();
    }
    assert_eq!(dc.depth(), 5);
    // Drop is safe: depth == base_depth
}

#[test]
fn dc_with_initial_depth_reset() {
    let mut dc = DepthCounter::with_initial_depth(10, 3);
    assert!(dc.enter());
    assert_eq!(dc.depth(), 4);
    dc.reset();
    assert_eq!(dc.depth(), 3); // resets to base, not 0
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "depth 0")]
fn dc_debug_leave_at_zero_panics() {
    let mut dc = DepthCounter::new(10);
    dc.leave(); // no matching enter
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "depth 0")]
fn dc_debug_double_leave_panics() {
    let mut dc = DepthCounter::new(10);
    assert!(dc.enter());
    dc.leave();
    dc.leave(); // second leave at depth 0
}
