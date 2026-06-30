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

use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};

/// Maximum global instantiation depth — bounds nesting of
/// `evaluate_application_type` calls across all `CheckerContext` instances.
/// Canonical definition in [`crate::limits`].
const MAX_GLOBAL_INSTANTIATION_DEPTH: u32 = crate::limits::MAX_GLOBAL_INSTANTIATION_DEPTH;

/// Maximum global instantiation fuel — limits TOTAL non-cached
/// `evaluate_application_type` invocations per file. React's react16.d.ts
/// can trigger thousands of unique Application evaluations; this caps work.
/// Canonical definition in [`crate::limits`].
const MAX_GLOBAL_INSTANTIATION_FUEL: u32 = crate::limits::MAX_GLOBAL_INSTANTIATION_FUEL;

/// Whether the shared evaluation session can enter another instantiation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationSessionLimitState {
    WithinLimits,
    DepthExceeded,
    FuelExhausted,
}

impl EvaluationSessionLimitState {
    pub const fn is_exceeded(self) -> bool {
        !matches!(self, Self::WithinLimits)
    }
}

/// Explicit evaluation session state.
///
/// Holds depth and fuel counters that must survive across `CheckerContext`
/// boundaries (cross-arena delegation creates child contexts with fresh
/// per-context counters, but the session counters are shared via `Rc`).
///
/// Uses `Cell` for interior mutability since all access is single-threaded.
#[derive(Default)]
pub struct EvaluationSession {
    /// Cross-context instantiation depth (nesting of `evaluate_application_type`).
    global_instantiation_depth: Cell<u32>,
    /// Cross-context instantiation fuel (total non-cached evaluations per file).
    global_instantiation_fuel: Cell<u32>,
    /// `TypeId`s currently expanded by fresh evaluators in this session.
    cross_eval_active: RefCell<FxHashSet<TypeId>>,
    /// Per-top-level-query memo for stable fresh-evaluator results.
    query_memo: RefCell<FxHashMap<(TypeId, bool), TypeId>>,
}

impl EvaluationSession {
    /// Create a new session with all counters at zero.
    pub fn new() -> Self {
        Self {
            global_instantiation_depth: Cell::new(0),
            global_instantiation_fuel: Cell::new(0),
            cross_eval_active: RefCell::new(FxHashSet::default()),
            query_memo: RefCell::new(FxHashMap::default()),
        }
    }

    /// Check which global instantiation limit, if any, is exceeded.
    #[inline]
    pub const fn instantiation_limit_state(&self) -> EvaluationSessionLimitState {
        if self.global_instantiation_depth.get() >= MAX_GLOBAL_INSTANTIATION_DEPTH {
            EvaluationSessionLimitState::DepthExceeded
        } else if self.global_instantiation_fuel.get() >= MAX_GLOBAL_INSTANTIATION_FUEL {
            EvaluationSessionLimitState::FuelExhausted
        } else {
            EvaluationSessionLimitState::WithinLimits
        }
    }

    /// Check if global instantiation limits are exceeded.
    #[inline]
    pub const fn instantiation_limits_exceeded(&self) -> bool {
        self.instantiation_limit_state().is_exceeded()
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

    /// Enter cross-evaluator expansion of `type_id`.
    ///
    /// Returns `false` when this session is already expanding the same type.
    #[inline]
    pub(crate) fn enter_cross_eval_type(&self, type_id: TypeId) -> bool {
        self.cross_eval_active.borrow_mut().insert(type_id)
    }

    /// Leave cross-evaluator expansion of `type_id`.
    #[inline]
    pub(crate) fn leave_cross_eval_type(&self, type_id: TypeId) {
        self.cross_eval_active.borrow_mut().remove(&type_id);
    }

    /// Look up a stable fresh-evaluator result for the current top-level query.
    #[inline]
    pub(crate) fn query_memo_get(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        self.query_memo
            .borrow()
            .get(&(type_id, no_unchecked_indexed_access))
            .copied()
    }

    /// Record a stable fresh-evaluator result for the current top-level query.
    #[inline]
    pub(crate) fn query_memo_put(
        &self,
        type_id: TypeId,
        no_unchecked_indexed_access: bool,
        result: TypeId,
    ) {
        self.query_memo
            .borrow_mut()
            .insert((type_id, no_unchecked_indexed_access), result);
    }

    /// Clear the per-query fresh-evaluator memo.
    #[inline]
    pub(crate) fn reset_query_memo(&self) {
        self.query_memo.borrow_mut().clear();
    }
}

thread_local! {
    static CURRENT_SESSION: EvaluationSession = EvaluationSession::new();
}

/// Borrow the current thread's default evaluation session.
pub(crate) fn with_current_session<T>(f: impl FnOnce(&EvaluationSession) -> T) -> T {
    CURRENT_SESSION.with(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new_has_zero_counters() {
        let session = EvaluationSession::new();
        assert_eq!(session.global_instantiation_depth(), 0);
        assert_eq!(session.global_instantiation_fuel(), 0);
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::WithinLimits
        );
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
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::DepthExceeded
        );
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
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::FuelExhausted
        );
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
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::WithinLimits
        );
        assert!(!session.instantiation_limits_exceeded());
    }

    #[test]
    fn test_depth_limit_is_primary_when_both_limits_exceeded() {
        let session = EvaluationSession::new();
        for _ in 0..MAX_GLOBAL_INSTANTIATION_FUEL {
            session.enter_instantiation();
        }

        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::DepthExceeded,
            "depth limit should stay the primary session limit once both limits are exceeded"
        );
    }

    #[test]
    fn test_cross_eval_active_set_is_session_owned() {
        let session = EvaluationSession::new();
        let type_id = TypeId(101);

        assert!(session.enter_cross_eval_type(type_id));
        assert!(
            !session.enter_cross_eval_type(type_id),
            "re-entering the same type in one session should be rejected"
        );
        session.leave_cross_eval_type(type_id);
        assert!(session.enter_cross_eval_type(type_id));
    }

    #[test]
    fn test_query_memo_keys_on_no_unchecked_indexed_access() {
        let session = EvaluationSession::new();
        let type_id = TypeId(202);

        session.query_memo_put(type_id, false, TypeId(210));
        session.query_memo_put(type_id, true, TypeId(211));

        assert_eq!(session.query_memo_get(type_id, false), Some(TypeId(210)));
        assert_eq!(session.query_memo_get(type_id, true), Some(TypeId(211)));

        session.reset_query_memo();
        assert_eq!(session.query_memo_get(type_id, false), None);
        assert_eq!(session.query_memo_get(type_id, true), None);
    }
}
