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

use crate::evaluation::request::EvaluationCacheKey;
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

/// Maximum re-entrant conditional-subtype relation depth.
const MAX_CONDITIONAL_SUBTYPE_DEPTH: u32 = crate::limits::MAX_CONDITIONAL_SUBTYPE_DEPTH;

/// Maximum infer-match fresh-evaluator expansion depth.
const MAX_INFER_MATCH_EXPANSION_DEPTH: u32 = crate::limits::MAX_INFER_MATCH_EXPANSION_DEPTH;

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

/// Whether infer-pattern matching may enter another fresh-evaluator expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InferMatchExpansionDepthState {
    LimitExceeded,
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
    /// Re-entrant conditional-subtype depth for
    /// `Evaluator -> SubtypeChecker -> Evaluator -> ...` chains.
    conditional_subtype_depth: Cell<u32>,
    /// Cross-evaluator nesting depth for infer-pattern matching expansion.
    infer_match_expansion_depth: Cell<u32>,
    /// `TypeId`s currently expanded by fresh evaluators in this session.
    cross_eval_active: RefCell<FxHashSet<TypeId>>,
    /// Per-top-level-query memo for stable fresh-evaluator results.
    query_memo: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
}

/// RAII entry for one conditional-subtype relation probe in an
/// [`EvaluationSession`].
#[must_use]
pub(crate) struct ConditionalSubtypeDepthEntry<'a> {
    session: &'a EvaluationSession,
    prior_depth: u32,
}

impl ConditionalSubtypeDepthEntry<'_> {
    pub(crate) const fn prior_depth(&self) -> u32 {
        self.prior_depth
    }

    pub(crate) const fn limit() -> u32 {
        MAX_CONDITIONAL_SUBTYPE_DEPTH
    }
}

impl Drop for ConditionalSubtypeDepthEntry<'_> {
    fn drop(&mut self) {
        self.session.conditional_subtype_depth.set(
            self.session
                .conditional_subtype_depth
                .get()
                .saturating_sub(1),
        );
    }
}

/// RAII entry for one infer-pattern fresh-evaluator expansion in an
/// [`EvaluationSession`].
#[must_use]
pub(crate) struct InferMatchExpansionDepthEntry<'a> {
    session: &'a EvaluationSession,
    #[cfg(test)]
    prior_depth: u32,
}

impl InferMatchExpansionDepthEntry<'_> {
    #[cfg(test)]
    pub(crate) const fn prior_depth(&self) -> u32 {
        self.prior_depth
    }

    #[cfg(test)]
    pub(crate) const fn limit() -> u32 {
        MAX_INFER_MATCH_EXPANSION_DEPTH
    }
}

impl Drop for InferMatchExpansionDepthEntry<'_> {
    fn drop(&mut self) {
        self.session.infer_match_expansion_depth.set(
            self.session
                .infer_match_expansion_depth
                .get()
                .saturating_sub(1),
        );
    }
}

impl EvaluationSession {
    /// Create a new session with all counters at zero.
    pub fn new() -> Self {
        Self {
            global_instantiation_depth: Cell::new(0),
            global_instantiation_fuel: Cell::new(0),
            conditional_subtype_depth: Cell::new(0),
            infer_match_expansion_depth: Cell::new(0),
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

    /// Enter a conditional-subtype probe and return the observed prior depth.
    #[inline]
    pub(crate) fn enter_conditional_subtype_depth(&self) -> ConditionalSubtypeDepthEntry<'_> {
        let prior_depth = self.conditional_subtype_depth.get();
        self.conditional_subtype_depth.set(prior_depth + 1);
        ConditionalSubtypeDepthEntry {
            session: self,
            prior_depth,
        }
    }

    /// Current re-entrant conditional-subtype depth.
    #[cfg(test)]
    #[inline]
    pub(crate) const fn conditional_subtype_depth(&self) -> u32 {
        self.conditional_subtype_depth.get()
    }

    /// Enter one infer-match fresh-evaluator expansion.
    #[inline]
    pub(crate) fn enter_infer_match_expansion_depth(
        &self,
    ) -> Result<InferMatchExpansionDepthEntry<'_>, InferMatchExpansionDepthState> {
        let prior_depth = self.infer_match_expansion_depth.get();
        if prior_depth >= MAX_INFER_MATCH_EXPANSION_DEPTH {
            return Err(InferMatchExpansionDepthState::LimitExceeded);
        }
        self.infer_match_expansion_depth.set(prior_depth + 1);
        Ok(InferMatchExpansionDepthEntry {
            session: self,
            #[cfg(test)]
            prior_depth,
        })
    }

    /// Current infer-match fresh-evaluator expansion depth.
    #[cfg(test)]
    #[inline]
    pub(crate) const fn infer_match_expansion_depth(&self) -> u32 {
        self.infer_match_expansion_depth.get()
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
    pub(crate) fn query_memo_get(&self, key: EvaluationCacheKey) -> Option<TypeId> {
        self.query_memo.borrow().get(&key).copied()
    }

    /// Record a stable fresh-evaluator result for the current top-level query.
    #[inline]
    pub(crate) fn query_memo_put(&self, key: EvaluationCacheKey, result: TypeId) {
        self.query_memo.borrow_mut().insert(key, result);
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
    fn test_query_memo_keys_on_index_access_options() {
        let session = EvaluationSession::new();
        let type_id = TypeId(202);
        let default_key = EvaluationCacheKey::new(type_id, false, false);
        let no_unchecked_key = EvaluationCacheKey::new(type_id, true, false);
        let exact_optional_key = EvaluationCacheKey::new(type_id, false, true);
        let both_key = EvaluationCacheKey::new(type_id, true, true);

        session.query_memo_put(default_key, TypeId(210));
        session.query_memo_put(no_unchecked_key, TypeId(211));
        session.query_memo_put(exact_optional_key, TypeId(212));

        assert_eq!(session.query_memo_get(default_key), Some(TypeId(210)));
        assert_eq!(session.query_memo_get(no_unchecked_key), Some(TypeId(211)));
        assert_eq!(
            session.query_memo_get(exact_optional_key),
            Some(TypeId(212))
        );
        assert_eq!(session.query_memo_get(both_key), None);

        session.reset_query_memo();
        assert_eq!(session.query_memo_get(default_key), None);
        assert_eq!(session.query_memo_get(no_unchecked_key), None);
        assert_eq!(session.query_memo_get(exact_optional_key), None);
    }

    #[test]
    fn conditional_subtype_depth_entry_restores_on_drop() {
        let session = EvaluationSession::new();
        assert_eq!(session.conditional_subtype_depth(), 0);

        {
            let entry = session.enter_conditional_subtype_depth();
            assert_eq!(entry.prior_depth(), 0);
            assert_eq!(session.conditional_subtype_depth(), 1);
        }

        assert_eq!(session.conditional_subtype_depth(), 0);
    }
}
