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
use rustc_hash::FxHashMap;
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

/// Maximum checker lazy-resolution fuel across all top-level calls in one
/// shared evaluation session.
const MAX_CHECKER_LAZY_RESOLUTION_FUEL: u32 = crate::limits::MAX_CHECKER_LAZY_RESOLUTION_FUEL;

/// Maximum nested checker application-symbol resolution calls.
const MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH: u32 =
    crate::limits::MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH;

/// Maximum local checker application-symbol resolution fuel.
const MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL: u32 =
    crate::limits::MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL;

/// Maximum checker refs-resolution prewalk fuel.
const MAX_CHECKER_REFS_RESOLUTION_FUEL: u32 = crate::limits::MAX_CHECKER_REFS_RESOLUTION_FUEL;

/// Maximum recursive checker env-evaluation depth.
const MAX_CHECKER_EVAL_ENV_DEPTH: u32 = crate::limits::MAX_CHECKER_EVAL_ENV_DEPTH;

/// Maximum re-entrant conditional-subtype relation depth.
const MAX_CONDITIONAL_SUBTYPE_DEPTH: u32 = crate::limits::MAX_CONDITIONAL_SUBTYPE_DEPTH;

/// Maximum infer-match fresh-evaluator expansion depth.
const MAX_INFER_MATCH_EXPANSION_DEPTH: u32 = crate::limits::MAX_INFER_MATCH_EXPANSION_DEPTH;

/// Maximum concurrent (in-flight) expansions of one `Application` node across
/// evaluator instances.
const MAX_CROSS_EVAL_APPLICATION_EXPANSION: u32 =
    crate::limits::MAX_CROSS_EVAL_APPLICATION_EXPANSION;

/// Counted in-flight registry of `TypeId`s being expanded across the
/// evaluator instances of one session.
///
/// One mechanism, two instances: the fresh-evaluator boundary roots
/// (`cross_eval_active`, limit 1 — the historical membership set) and the
/// nested `Application` sentinel (`application_expansion_active`, limit
/// [`MAX_CROSS_EVAL_APPLICATION_EXPANSION`]) share this counter so the two
/// "defer to the in-flight owner" policies cannot drift apart.
#[derive(Default)]
struct InFlightTypeCounter {
    active: RefCell<FxHashMap<TypeId, u32>>,
}

impl InFlightTypeCounter {
    /// Record one in-flight expansion of `node` unless `limit` expansions
    /// are already active. A `true` return must be balanced by
    /// [`Self::leave`].
    fn enter(&self, node: TypeId, limit: u32) -> bool {
        let mut active = self.active.borrow_mut();
        let count = active.entry(node).or_insert(0);
        if *count >= limit {
            return false;
        }
        *count += 1;
        true
    }

    /// Balance one successful [`Self::enter`] of `node`. Entries drop out of
    /// the map at zero so the thread-lifetime default session never
    /// accumulates dead keys.
    fn leave(&self, node: TypeId) {
        use std::collections::hash_map::Entry;
        if let Entry::Occupied(mut entry) = self.active.borrow_mut().entry(node) {
            if *entry.get() <= 1 {
                entry.remove();
            } else {
                *entry.get_mut() -= 1;
            }
        }
    }
}

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

/// RAII entry for one checker env-evaluation expansion.
#[must_use]
pub struct EvalEnvDepthEntry<'a> {
    session: &'a EvaluationSession,
    prior_depth: u32,
}

impl EvalEnvDepthEntry<'_> {
    pub const fn prior_depth(&self) -> u32 {
        self.prior_depth
    }
}

impl Drop for EvalEnvDepthEntry<'_> {
    fn drop(&mut self) {
        self.session.checker_eval_env_depth.set(self.prior_depth);
    }
}

/// RAII entry for one checker application-symbol resolution expansion.
#[must_use]
pub struct AppSymbolResolutionDepthEntry<'a> {
    session: &'a EvaluationSession,
    prior_depth: u32,
    outermost: bool,
}

impl AppSymbolResolutionDepthEntry<'_> {
    pub const fn outermost(&self) -> bool {
        self.outermost
    }
}

impl Drop for AppSymbolResolutionDepthEntry<'_> {
    fn drop(&mut self) {
        self.session
            .checker_app_symbol_resolution_depth
            .set(self.prior_depth);
    }
}

/// RAII scope for checker refs-resolution prewalk fuel.
#[must_use]
pub struct RefsResolutionScope<'a> {
    session: &'a EvaluationSession,
    outermost: bool,
}

impl RefsResolutionScope<'_> {
    pub const fn outermost(&self) -> bool {
        self.outermost
    }
}

impl Drop for RefsResolutionScope<'_> {
    fn drop(&mut self) {
        if self.outermost {
            self.session.checker_refs_resolution_active.set(false);
        }
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
    /// Cross-context checker lazy-resolution fuel.
    lazy_resolution_fuel: Cell<u32>,
    /// Checker application-symbol resolution depth.
    checker_app_symbol_resolution_depth: Cell<u32>,
    /// Checker application-symbol resolution local fuel.
    checker_app_symbol_resolution_fuel: Cell<u32>,
    /// Checker refs-resolution prewalk local fuel.
    checker_refs_resolution_fuel: Cell<u32>,
    /// Whether a checker refs-resolution prewalk is active.
    checker_refs_resolution_active: Cell<bool>,
    /// Checker env-evaluation recursive depth.
    checker_eval_env_depth: Cell<u32>,
    /// Re-entrant conditional-subtype depth for
    /// `Evaluator -> SubtypeChecker -> Evaluator -> ...` chains.
    conditional_subtype_depth: Cell<u32>,
    /// Cross-evaluator nesting depth for infer-pattern matching expansion.
    infer_match_expansion_depth: Cell<u32>,
    /// Checker type-reference alias-forwarding depth.
    type_reference_resolution_depth: Cell<u32>,
    /// `TypeId`s currently expanded by fresh evaluators in this session.
    cross_eval_active: InFlightTypeCounter,
    /// Per-top-level-query memo for stable fresh-evaluator results.
    query_memo: RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    /// In-flight expansion count per `Application` node across every
    /// evaluator instance in this session. A fresh evaluator re-entering a
    /// node that [`MAX_CROSS_EVAL_APPLICATION_EXPANSION`] instances are
    /// already expanding must defer to the in-flight owner instead of
    /// re-walking the alias graph (issue #13508 root cause B).
    application_expansion_active: InFlightTypeCounter,
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

/// RAII entry for one checker type-reference alias-forwarding expansion.
#[must_use]
pub struct TypeReferenceResolutionDepthEntry<'a> {
    session: &'a EvaluationSession,
}

impl Drop for TypeReferenceResolutionDepthEntry<'_> {
    fn drop(&mut self) {
        self.session.type_reference_resolution_depth.set(
            self.session
                .type_reference_resolution_depth
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
            lazy_resolution_fuel: Cell::new(0),
            checker_app_symbol_resolution_depth: Cell::new(0),
            checker_app_symbol_resolution_fuel: Cell::new(0),
            checker_refs_resolution_fuel: Cell::new(0),
            checker_refs_resolution_active: Cell::new(false),
            checker_eval_env_depth: Cell::new(0),
            conditional_subtype_depth: Cell::new(0),
            infer_match_expansion_depth: Cell::new(0),
            type_reference_resolution_depth: Cell::new(0),
            cross_eval_active: InFlightTypeCounter::default(),
            query_memo: RefCell::new(FxHashMap::default()),
            application_expansion_active: InFlightTypeCounter::default(),
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

    /// Check if checker lazy-resolution fuel is exhausted.
    #[inline]
    pub const fn lazy_resolution_fuel_exhausted(&self) -> bool {
        self.lazy_resolution_fuel.get() >= MAX_CHECKER_LAZY_RESOLUTION_FUEL
    }

    /// Increment checker lazy-resolution fuel.
    #[inline]
    pub fn increment_lazy_resolution_fuel(&self) {
        self.lazy_resolution_fuel
            .set(self.lazy_resolution_fuel.get() + 1);
    }

    /// Reset checker lazy-resolution fuel for a new file, statement, or retry boundary.
    #[inline]
    pub fn reset_lazy_resolution_fuel(&self) {
        self.lazy_resolution_fuel.set(0);
    }

    /// Read checker lazy-resolution fuel for snapshot/restore.
    #[inline]
    pub const fn lazy_resolution_fuel_value(&self) -> u32 {
        self.lazy_resolution_fuel.get()
    }

    /// Restore checker lazy-resolution fuel to a previously captured value.
    #[inline]
    pub fn restore_lazy_resolution_fuel(&self, value: u32) {
        self.lazy_resolution_fuel.set(value);
    }

    /// Reset checker lazy-readiness guards for a new file or statement boundary.
    #[inline]
    pub fn reset_lazy_readiness_guards(&self) {
        self.checker_app_symbol_resolution_depth.set(0);
        self.checker_app_symbol_resolution_fuel.set(0);
        self.checker_refs_resolution_fuel.set(0);
        self.checker_refs_resolution_active.set(false);
        self.checker_eval_env_depth.set(0);
    }

    /// Enter checker env evaluation, returning `None` when the depth cap is hit.
    #[inline]
    pub fn enter_eval_env_depth(&self) -> Option<EvalEnvDepthEntry<'_>> {
        let prior_depth = self.checker_eval_env_depth.get();
        if prior_depth >= MAX_CHECKER_EVAL_ENV_DEPTH {
            None
        } else {
            self.checker_eval_env_depth.set(prior_depth + 1);
            Some(EvalEnvDepthEntry {
                session: self,
                prior_depth,
            })
        }
    }

    /// Current checker env-evaluation depth.
    #[inline]
    pub const fn eval_env_depth(&self) -> u32 {
        self.checker_eval_env_depth.get()
    }

    /// Checker env-evaluation depth limit.
    #[inline]
    pub const fn eval_env_depth_limit(&self) -> u32 {
        MAX_CHECKER_EVAL_ENV_DEPTH
    }

    /// Current checker application-symbol resolution depth.
    #[inline]
    pub const fn app_symbol_resolution_depth(&self) -> u32 {
        self.checker_app_symbol_resolution_depth.get()
    }

    /// Checker application-symbol resolution depth limit.
    #[inline]
    pub const fn app_symbol_resolution_depth_limit(&self) -> u32 {
        MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH
    }

    /// Current checker application-symbol resolution local fuel.
    #[inline]
    pub const fn app_symbol_resolution_fuel(&self) -> u32 {
        self.checker_app_symbol_resolution_fuel.get()
    }

    /// Checker application-symbol resolution local fuel limit.
    #[inline]
    pub const fn app_symbol_resolution_fuel_limit(&self) -> u32 {
        MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL
    }

    /// Enter checker application-symbol resolution.
    #[inline]
    pub fn enter_app_symbol_resolution_depth(&self) -> AppSymbolResolutionDepthEntry<'_> {
        let prior_depth = self.checker_app_symbol_resolution_depth.get();
        self.checker_app_symbol_resolution_depth
            .set(prior_depth + 1);
        AppSymbolResolutionDepthEntry {
            session: self,
            prior_depth,
            outermost: prior_depth == 0,
        }
    }

    /// Reset checker application-symbol resolution local fuel.
    #[inline]
    pub fn reset_app_symbol_resolution_fuel(&self) {
        self.checker_app_symbol_resolution_fuel.set(0);
    }

    /// Increment checker application-symbol resolution local fuel.
    #[inline]
    pub fn increment_app_symbol_resolution_fuel(&self) {
        self.checker_app_symbol_resolution_fuel
            .set(self.checker_app_symbol_resolution_fuel.get() + 1);
    }

    /// Whether checker application-symbol resolution local fuel is exhausted.
    #[inline]
    pub const fn app_symbol_resolution_fuel_exhausted(&self) -> bool {
        self.checker_app_symbol_resolution_fuel.get() >= MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL
    }

    /// Enter a checker refs-resolution prewalk scope.
    #[inline]
    pub fn enter_refs_resolution_scope(&self) -> RefsResolutionScope<'_> {
        let outermost = !self.checker_refs_resolution_active.get();
        if outermost {
            self.checker_refs_resolution_active.set(true);
            self.checker_refs_resolution_fuel.set(0);
        }
        RefsResolutionScope {
            session: self,
            outermost,
        }
    }

    /// Whether checker refs-resolution prewalk local fuel is exhausted.
    #[inline]
    pub const fn refs_resolution_fuel_exhausted(&self) -> bool {
        self.checker_refs_resolution_fuel.get() >= MAX_CHECKER_REFS_RESOLUTION_FUEL
    }

    /// Current checker refs-resolution prewalk local fuel.
    #[inline]
    pub const fn refs_resolution_fuel(&self) -> u32 {
        self.checker_refs_resolution_fuel.get()
    }

    /// Checker refs-resolution prewalk local fuel limit.
    #[inline]
    pub const fn refs_resolution_fuel_limit(&self) -> u32 {
        MAX_CHECKER_REFS_RESOLUTION_FUEL
    }

    /// Increment checker refs-resolution prewalk local fuel.
    #[inline]
    pub fn increment_refs_resolution_fuel(&self) {
        self.checker_refs_resolution_fuel
            .set(self.checker_refs_resolution_fuel.get() + 1);
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

    /// Enter one checker type-reference alias-forwarding expansion.
    #[inline]
    pub fn enter_type_reference_resolution_depth(
        &self,
    ) -> Option<TypeReferenceResolutionDepthEntry<'_>> {
        let prior_depth = self.type_reference_resolution_depth.get();
        if prior_depth >= crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH {
            None
        } else {
            self.type_reference_resolution_depth.set(prior_depth + 1);
            Some(TypeReferenceResolutionDepthEntry { session: self })
        }
    }

    /// Current checker type-reference alias-forwarding depth.
    #[inline]
    pub const fn type_reference_resolution_depth(&self) -> u32 {
        self.type_reference_resolution_depth.get()
    }

    /// Reset checker type-reference alias-forwarding depth for a new file.
    #[inline]
    pub fn reset_type_reference_resolution_depth(&self) {
        self.type_reference_resolution_depth.set(0);
    }

    /// Enter cross-evaluator expansion of `type_id`.
    ///
    /// Returns `false` when this session is already expanding the same type.
    #[inline]
    pub(crate) fn enter_cross_eval_type(&self, type_id: TypeId) -> bool {
        self.cross_eval_active.enter(type_id, 1)
    }

    /// Leave cross-evaluator expansion of `type_id`.
    #[inline]
    pub(crate) fn leave_cross_eval_type(&self, type_id: TypeId) {
        self.cross_eval_active.leave(type_id);
    }

    /// Enter one cross-evaluator expansion of an `Application` node.
    ///
    /// Returns `false` when [`MAX_CROSS_EVAL_APPLICATION_EXPANSION`]
    /// evaluator instances in this session are already expanding the same
    /// node — the caller must then defer (keep the application opaque) and
    /// let the in-flight owner produce the result. A `true` return records
    /// the entry and must be balanced by
    /// [`leave_application_expansion`](Self::leave_application_expansion).
    #[inline]
    pub(crate) fn enter_application_expansion(&self, node: TypeId) -> bool {
        self.application_expansion_active
            .enter(node, MAX_CROSS_EVAL_APPLICATION_EXPANSION)
    }

    /// Leave one cross-evaluator expansion of an `Application` node.
    /// Balanced with a successful
    /// [`enter_application_expansion`](Self::enter_application_expansion).
    #[inline]
    pub(crate) fn leave_application_expansion(&self, node: TypeId) {
        self.application_expansion_active.leave(node);
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
    fn test_lazy_resolution_fuel_snapshot_restore_and_limit() {
        let session = EvaluationSession::new();
        assert_eq!(session.lazy_resolution_fuel_value(), 0);
        assert!(!session.lazy_resolution_fuel_exhausted());

        session.increment_lazy_resolution_fuel();
        assert_eq!(session.lazy_resolution_fuel_value(), 1);

        session.restore_lazy_resolution_fuel(MAX_CHECKER_LAZY_RESOLUTION_FUEL);
        assert!(session.lazy_resolution_fuel_exhausted());

        session.reset_lazy_resolution_fuel();
        assert_eq!(session.lazy_resolution_fuel_value(), 0);
        assert!(!session.lazy_resolution_fuel_exhausted());
    }

    #[test]
    fn checker_eval_env_depth_entry_restores_on_drop_and_rejects_at_cap() {
        let session = EvaluationSession::new();
        let mut entries = Vec::new();
        for expected_prior in 0..MAX_CHECKER_EVAL_ENV_DEPTH {
            let entry = session
                .enter_eval_env_depth()
                .expect("pre-cap env-eval depth entry should fit");
            assert_eq!(entry.prior_depth(), expected_prior);
            entries.push(entry);
        }

        assert_eq!(session.eval_env_depth(), MAX_CHECKER_EVAL_ENV_DEPTH);
        assert!(session.enter_eval_env_depth().is_none());
        while let Some(entry) = entries.pop() {
            drop(entry);
        }
        assert_eq!(session.eval_env_depth(), 0);
    }

    #[test]
    fn checker_app_symbol_resolution_depth_and_fuel_are_session_owned() {
        let session = EvaluationSession::new();
        {
            let entry = session.enter_app_symbol_resolution_depth();
            assert!(entry.outermost());
            assert_eq!(session.app_symbol_resolution_depth(), 1);
            let nested = session.enter_app_symbol_resolution_depth();
            assert!(!nested.outermost());
            assert_eq!(session.app_symbol_resolution_depth(), 2);
        }
        assert_eq!(session.app_symbol_resolution_depth(), 0);

        session.increment_app_symbol_resolution_fuel();
        assert_eq!(session.app_symbol_resolution_fuel(), 1);
        session.reset_app_symbol_resolution_fuel();
        assert_eq!(session.app_symbol_resolution_fuel(), 0);
        for _ in 0..MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL {
            session.increment_app_symbol_resolution_fuel();
        }
        assert!(session.app_symbol_resolution_fuel_exhausted());
    }

    #[test]
    fn checker_refs_resolution_scope_resets_outermost_fuel_and_restores_active() {
        let session = EvaluationSession::new();
        {
            let outer = session.enter_refs_resolution_scope();
            assert!(outer.outermost());
            session.increment_refs_resolution_fuel();
            assert_eq!(session.refs_resolution_fuel(), 1);
            {
                let nested = session.enter_refs_resolution_scope();
                assert!(!nested.outermost());
                assert_eq!(session.refs_resolution_fuel(), 1);
            }
            assert_eq!(session.refs_resolution_fuel(), 1);
        }

        let new_outer = session.enter_refs_resolution_scope();
        assert!(new_outer.outermost());
        assert_eq!(
            session.refs_resolution_fuel(),
            0,
            "a new outer refs-resolution scope should reset local prewalk fuel"
        );
        for _ in 0..MAX_CHECKER_REFS_RESOLUTION_FUEL {
            session.increment_refs_resolution_fuel();
        }
        assert!(session.refs_resolution_fuel_exhausted());
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
    fn application_expansion_sentinel_defers_at_limit_and_rebalances() {
        let session = EvaluationSession::new();
        let node = TypeId(4321);

        let mut entered = 0;
        while session.enter_application_expansion(node) {
            entered += 1;
            assert!(
                entered <= MAX_CROSS_EVAL_APPLICATION_EXPANSION,
                "enter must deny past the in-flight expansion limit"
            );
        }
        assert_eq!(entered, MAX_CROSS_EVAL_APPLICATION_EXPANSION);
        assert!(
            !session.enter_application_expansion(node),
            "an at-limit node must keep deferring until an owner leaves"
        );

        session.leave_application_expansion(node);
        assert!(
            session.enter_application_expansion(node),
            "leaving one expansion frees one re-entry slot"
        );
        for _ in 0..entered {
            session.leave_application_expansion(node);
        }
        assert!(
            session.enter_application_expansion(node),
            "a fully-unwound node is enterable again"
        );
    }

    #[test]
    fn application_expansion_sentinel_tracks_nodes_independently() {
        let session = EvaluationSession::new();
        let hot = TypeId(11);
        let other = TypeId(12);

        while session.enter_application_expansion(hot) {}
        assert!(
            session.enter_application_expansion(other),
            "an at-limit node must not defer expansions of a different node"
        );
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

    #[test]
    fn type_reference_resolution_depth_entry_restores_on_drop() {
        let session = EvaluationSession::new();
        {
            let _outer = session
                .enter_type_reference_resolution_depth()
                .expect("first type-reference depth entry should fit");
            assert_eq!(session.type_reference_resolution_depth(), 1);
            {
                let _inner = session
                    .enter_type_reference_resolution_depth()
                    .expect("nested type-reference depth entry should fit");
                assert_eq!(session.type_reference_resolution_depth(), 2);
            }
            assert_eq!(session.type_reference_resolution_depth(), 1);
        }
        assert_eq!(session.type_reference_resolution_depth(), 0);
    }

    #[test]
    fn type_reference_resolution_depth_rejects_at_cap_without_mutating_depth() {
        let session = EvaluationSession::new();
        let mut entries = Vec::new();
        for _ in 0..crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH {
            entries.push(
                session
                    .enter_type_reference_resolution_depth()
                    .expect("pre-cap entry should fit"),
            );
        }

        assert_eq!(
            session.type_reference_resolution_depth(),
            crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH
        );
        assert!(session.enter_type_reference_resolution_depth().is_none());
        assert_eq!(
            session.type_reference_resolution_depth(),
            crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH
        );
        drop(entries);
        assert_eq!(session.type_reference_resolution_depth(), 0);
    }
}
