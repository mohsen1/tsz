//! Unified recursion guard for cycle detection, depth limiting,
//! and iteration bounding in recursive type computations.
//!
//! Replaces the scattered `in_progress` / `visiting` / `depth` / `total_checks`
//! fields that were manually reimplemented in SubtypeChecker, TypeEvaluator,
//! and PropertyAccessEvaluator with subtle variations.

use rustc_hash::FxHashSet;
use std::hash::Hash;

/// Result of attempting to enter a recursive computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursionResult {
    /// Proceed with the computation.
    Entered,
    /// This key is already being visited -- cycle detected.
    Cycle,
    /// Maximum recursion depth exceeded.
    DepthExceeded,
    /// Maximum iteration count exceeded.
    IterationExceeded,
}

/// Tracks recursion state for cycle detection, depth limiting,
/// and iteration bounding.
///
/// # Usage
///
/// ```ignore
/// let mut guard = RecursionGuard::new(100, 100_000);
///
/// match guard.enter(key) {
///     RecursionResult::Entered => {
///         let result = do_work();
///         guard.leave(key);
///         result
///     }
///     RecursionResult::Cycle => handle_cycle(),
///     RecursionResult::DepthExceeded |
///     RecursionResult::IterationExceeded => handle_exceeded(),
/// }
/// ```
pub struct RecursionGuard<K: Hash + Eq + Copy> {
    visiting: FxHashSet<K>,
    depth: u32,
    iterations: u32,
    max_depth: u32,
    max_iterations: u32,
    max_visiting: u32,
    pub depth_exceeded: bool,
}

impl<K: Hash + Eq + Copy> RecursionGuard<K> {
    pub fn new(max_depth: u32, max_iterations: u32) -> Self {
        Self {
            visiting: FxHashSet::default(),
            depth: 0,
            iterations: 0,
            max_depth,
            max_iterations,
            max_visiting: 10_000,
            depth_exceeded: false,
        }
    }

    /// Create a guard with a custom max visiting set size.
    pub fn with_max_visiting(mut self, max_visiting: u32) -> Self {
        self.max_visiting = max_visiting;
        self
    }

    /// Try to enter a recursive computation for `key`.
    /// Returns `Entered` if OK, or the reason we can't proceed.
    pub fn enter(&mut self, key: K) -> RecursionResult {
        self.iterations += 1;
        if self.iterations > self.max_iterations {
            self.depth_exceeded = true;
            return RecursionResult::IterationExceeded;
        }
        if self.depth >= self.max_depth {
            self.depth_exceeded = true;
            return RecursionResult::DepthExceeded;
        }
        if self.visiting.contains(&key) {
            return RecursionResult::Cycle;
        }
        if self.visiting.len() as u32 >= self.max_visiting {
            self.depth_exceeded = true;
            return RecursionResult::DepthExceeded;
        }
        self.visiting.insert(key);
        self.depth += 1;
        RecursionResult::Entered
    }

    /// Leave a recursive computation for `key`.
    /// MUST be called after every successful `enter()`.
    pub fn leave(&mut self, key: K) {
        self.visiting.remove(&key);
        self.depth = self.depth.saturating_sub(1);
    }

    /// Check if `key` is currently being visited (without entering).
    pub fn is_visiting(&self, key: &K) -> bool {
        self.visiting.contains(key)
    }

    /// Reset all state while preserving limits.
    pub fn reset(&mut self) {
        self.visiting.clear();
        self.depth = 0;
        self.iterations = 0;
        self.depth_exceeded = false;
    }

    /// Current recursion depth.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Total iterations so far.
    pub fn iterations(&self) -> u32 {
        self.iterations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_enter_leave() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.depth(), 1);
        assert!(guard.is_visiting(&1));
        guard.leave(1);
        assert_eq!(guard.depth(), 0);
        assert!(!guard.is_visiting(&1));
    }

    #[test]
    fn test_cycle_detection() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
        guard.leave(1);
    }

    #[test]
    fn test_depth_exceeded() {
        let mut guard = RecursionGuard::new(2, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(2u32), RecursionResult::Entered);
        assert_eq!(guard.enter(3u32), RecursionResult::DepthExceeded);
        assert!(guard.depth_exceeded);
        guard.leave(2);
        guard.leave(1);
    }

    #[test]
    fn test_iteration_exceeded() {
        let mut guard = RecursionGuard::new(100, 3);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        guard.leave(1);
        assert_eq!(guard.enter(2u32), RecursionResult::Entered);
        guard.leave(2);
        assert_eq!(guard.enter(3u32), RecursionResult::Entered);
        guard.leave(3);
        assert_eq!(guard.enter(4u32), RecursionResult::IterationExceeded);
    }

    #[test]
    fn test_nested_different_keys() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(2u32), RecursionResult::Entered);
        assert_eq!(guard.enter(3u32), RecursionResult::Entered);
        assert_eq!(guard.depth(), 3);
        assert!(guard.is_visiting(&1));
        assert!(guard.is_visiting(&2));
        assert!(guard.is_visiting(&3));
        guard.leave(3);
        guard.leave(2);
        guard.leave(1);
        assert_eq!(guard.depth(), 0);
        assert_eq!(guard.iterations(), 3);
    }

    #[test]
    fn test_reenter_after_leave() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        guard.leave(1);
        // Same key should be enterable again after leaving
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.depth(), 1);
        guard.leave(1);
    }

    #[test]
    fn test_reset_clears_all_state() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(2u32), RecursionResult::Entered);
        guard.depth_exceeded = true;
        guard.reset();
        assert_eq!(guard.depth(), 0);
        assert_eq!(guard.iterations(), 0);
        assert!(!guard.depth_exceeded);
        assert!(!guard.is_visiting(&1));
        assert!(!guard.is_visiting(&2));
        // Should be able to enter again after reset
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
    }

    #[test]
    fn test_max_visiting_set_size() {
        let mut guard = RecursionGuard::new(1000, 100_000).with_max_visiting(3);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(2u32), RecursionResult::Entered);
        assert_eq!(guard.enter(3u32), RecursionResult::Entered);
        // 4th entry should fail: visiting set is at capacity
        assert_eq!(guard.enter(4u32), RecursionResult::DepthExceeded);
        assert!(guard.depth_exceeded);
    }

    #[test]
    fn test_cycle_does_not_set_depth_exceeded() {
        let mut guard = RecursionGuard::new(10, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
        // Cycle detection should NOT set depth_exceeded
        assert!(!guard.depth_exceeded);
        guard.leave(1);
    }

    #[test]
    fn test_tuple_keys() {
        // Test with (TypeId, TypeId)-like tuple keys (as used by SubtypeChecker)
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
    fn test_depth_exceeded_persists_across_calls() {
        let mut guard = RecursionGuard::new(1, 100);
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        // depth = 1, max = 1, next enter should fail
        assert_eq!(guard.enter(2u32), RecursionResult::DepthExceeded);
        assert!(guard.depth_exceeded);
        guard.leave(1);
        // depth_exceeded stays true even after leaving
        assert!(guard.depth_exceeded);
    }

    #[test]
    fn test_iterations_count_all_attempts() {
        let mut guard = RecursionGuard::new(10, 100);
        // Successful enters count
        assert_eq!(guard.enter(1u32), RecursionResult::Entered);
        assert_eq!(guard.iterations(), 1);
        // Cycle detection also counts
        assert_eq!(guard.enter(1u32), RecursionResult::Cycle);
        assert_eq!(guard.iterations(), 2);
        guard.leave(1);
        assert_eq!(guard.iterations(), 2); // leave doesn't change iterations
    }
}
