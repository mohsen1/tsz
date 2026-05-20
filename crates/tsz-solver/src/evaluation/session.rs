//! Explicit evaluation session state that replaces thread-local depth/fuel guards.
//!
//! An `EvaluationSession` tracks cumulative evaluation work across multiple
//! `TypeEvaluator` instances and cross-arena `CheckerContext` boundaries.
//! Previously, this state was held in `thread_local!` counters which were
//! invisible, hard to test, and prevented future multi-threaded evaluation.
//!
//! The session is created at the top-level entry point (checker) and shared
//! via `Rc` across parent/child contexts so counters survive cross-arena
//! delegation without implicit global state.

use std::cell::Cell;

/// Maximum global instantiation depth — bounds nesting of
/// `evaluate_application_type` calls across all `CheckerContext` instances.
const MAX_GLOBAL_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum global instantiation fuel — limits TOTAL non-cached
/// `evaluate_application_type` invocations per file. React's react16.d.ts
/// can trigger thousands of unique Application evaluations; this caps work.
const MAX_GLOBAL_INSTANTIATION_FUEL: u32 = 2000;

/// Explicit evaluation session state.
///
/// Holds depth and fuel counters that must survive across `CheckerContext`
/// boundaries (cross-arena delegation creates child contexts with fresh
/// per-context counters, but the session counters are shared via `Rc`).
///
/// Uses `Cell` for interior mutability since all access is single-threaded.
pub struct EvaluationSession {
    /// Cross-context instantiation depth (nesting of `evaluate_application_type`).
    global_instantiation_depth: Cell<u32>,
    /// Cross-context instantiation fuel (total non-cached evaluations per file).
    global_instantiation_fuel: Cell<u32>,
}

impl EvaluationSession {
    /// Create a new session with all counters at zero.
    pub const fn new() -> Self {
        Self {
            global_instantiation_depth: Cell::new(0),
            global_instantiation_fuel: Cell::new(0),
        }
    }

    /// Check if global instantiation limits are exceeded.
    #[inline]
    pub const fn instantiation_limits_exceeded(&self) -> bool {
        self.global_instantiation_depth.get() >= MAX_GLOBAL_INSTANTIATION_DEPTH
            || self.global_instantiation_fuel.get() >= MAX_GLOBAL_INSTANTIATION_FUEL
    }

    /// Increment both instantiation depth and fuel before an evaluation.
    /// Returns the previous depth (for restoring on exit).
    #[inline]
    pub fn enter_instantiation(&self) -> u32 {
        let prev_depth = self.global_instantiation_depth.get();
        self.global_instantiation_depth.set(prev_depth + 1);
        self.global_instantiation_fuel
            .set(self.global_instantiation_fuel.get() + 1);
        prev_depth
    }

    /// Decrement instantiation depth after an evaluation completes.
    #[inline]
    pub fn leave_instantiation(&self) {
        self.global_instantiation_depth
            .set(self.global_instantiation_depth.get().saturating_sub(1));
    }

    /// Reset instantiation fuel for a new file. Each file gets a fresh budget.
    #[inline]
    pub fn reset_instantiation_fuel(&self) {
        self.global_instantiation_fuel.set(0);
    }

    /// Get the current global instantiation depth (for diagnostics/testing).
    #[inline]
    pub const fn global_instantiation_depth(&self) -> u32 {
        self.global_instantiation_depth.get()
    }

    /// Get the current global instantiation fuel (for diagnostics/testing).
    #[inline]
    pub const fn global_instantiation_fuel(&self) -> u32 {
        self.global_instantiation_fuel.get()
    }
}

impl Default for EvaluationSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new_has_zero_counters() {
        let session = EvaluationSession::new();
        assert_eq!(session.global_instantiation_depth(), 0);
        assert_eq!(session.global_instantiation_fuel(), 0);
        assert!(!session.instantiation_limits_exceeded());
    }

    #[test]
    fn test_enter_leave_instantiation() {
        let session = EvaluationSession::new();
        let prev = session.enter_instantiation();
        assert_eq!(prev, 0);
        assert_eq!(session.global_instantiation_depth(), 1);
        assert_eq!(session.global_instantiation_fuel(), 1);

        session.leave_instantiation();
        assert_eq!(session.global_instantiation_depth(), 0);
        // Fuel does not decrement
        assert_eq!(session.global_instantiation_fuel(), 1);
    }

    #[test]
    fn test_depth_limit_exceeded() {
        let session = EvaluationSession::new();
        for _ in 0..MAX_GLOBAL_INSTANTIATION_DEPTH {
            session.enter_instantiation();
        }
        assert!(session.instantiation_limits_exceeded());
    }

    #[test]
    fn test_fuel_limit_exceeded() {
        let session = EvaluationSession::new();
        // Enter and leave repeatedly to exhaust fuel without hitting depth limit
        for _ in 0..MAX_GLOBAL_INSTANTIATION_FUEL {
            session.enter_instantiation();
            session.leave_instantiation();
        }
        assert!(session.instantiation_limits_exceeded());
    }

    #[test]
    fn test_reset_instantiation_fuel() {
        let session = EvaluationSession::new();
        for _ in 0..10 {
            session.enter_instantiation();
            session.leave_instantiation();
        }
        assert_eq!(session.global_instantiation_fuel(), 10);
        session.reset_instantiation_fuel();
        assert_eq!(session.global_instantiation_fuel(), 0);
        assert!(!session.instantiation_limits_exceeded());
    }
}
