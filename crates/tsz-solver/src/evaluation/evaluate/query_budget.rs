//! Cross-instance per-query operation budget for [`TypeEvaluator::evaluate`].
//!
//! [`TypeEvaluator::evaluate`]: super::TypeEvaluator::evaluate
//!
//! `MAX_GLOBAL_EVAL_DEPTH` bounds the live call-*stack* depth, but some recursive
//! type families never grow a deep stack: conditional and `infer`-pattern
//! evaluation spin up *fresh* `TypeEvaluator` / `SubtypeChecker` instances
//! mid-relation, each with per-instance cycle/depth/iteration guards reset to
//! zero, and bounce between a handful of types whose identity keeps changing
//! (fresh `infer` placeholders / object shapes), so no per-instance guard ever
//! fires. A recursive generic wrapper applied to a literal/object argument —
//! `type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T` over `Box<2>`, or the
//! standard-library `Awaited<Promise<2>>` — hangs the compile this way.
//!
//! This module provides the cross-instance work bound that mirrors tsc's global
//! `instantiationCount`: [`EvalQueryFrame`] counts *every* `evaluate` operation
//! across all instances within one top-level query (reset when the outermost
//! frame begins) and reports exhaustion so `evaluate` can bail, terminating the
//! runaway regardless of which boundary it bounces through.

use super::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use std::cell::Cell;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Enter one operation of the cross-instance per-query budget.
    ///
    /// Returns the live [`EvalQueryFrame`] guard while within budget. When the
    /// budget is exhausted it flags the evaluator's recursion-bail state
    /// (`deep_recursion_seen` / `silent_depth_bailed`, matching the structural
    /// depth-bailout arms) and returns `None`, signalling the caller to leave the
    /// type opaque so the cross-instance runaway unwinds instead of hanging.
    pub(super) fn enter_eval_query_budget(&mut self) -> Option<EvalQueryFrame> {
        let frame = EvalQueryFrame::enter(resolved_max_eval_ops());
        if frame.budget_exhausted {
            self.mark_deep_recursion_seen();
            self.mark_silent_depth_bailed();
            return None;
        }
        Some(frame)
    }
}

thread_local! {
    /// Live count of nested `evaluate` frames across *all* `TypeEvaluator`
    /// instances on the current thread. A value of `0` means no evaluation is in
    /// flight, so the next `evaluate` begins a fresh top-level query.
    static EVAL_QUERY_ACTIVE: Cell<u32> = const { Cell::new(0) };
    /// Total `evaluate` operations performed in the current top-level query.
    /// Reset whenever `EVAL_QUERY_ACTIVE` transitions from `0`, so the budget is
    /// per top-level query and never carries over to poison sibling type
    /// positions. See [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`].
    static EVAL_QUERY_OPS: Cell<u32> = const { Cell::new(0) };
}

/// Total `evaluate` operations permitted for a single top-level evaluation query
/// (the outermost `evaluate` call on the thread, before it returns).
///
/// Defaults to the whole-file `MAX_EVALUATION_FUEL` (`2_000_000`): a single
/// top-level query that out-works the entire file's evaluation-fuel budget is,
/// by construction, a runaway. This keeps the guard strictly weaker than the
/// existing whole-file fuel for terminating evaluations (which never approach
/// it), so it changes behaviour only for the cross-instance recursions that the
/// fuel's per-128-iteration sampling fails to observe.
///
/// Overridable via the `TSZ_MAX_EVAL_OPS` environment variable, which tests use
/// to force the bail quickly without a multi-million-op spin. See
/// [`resolved_max_eval_ops`].
pub(super) const DEFAULT_MAX_EVAL_OPS_PER_QUERY: u32 = 2_000_000;

/// Resolve the per-query `evaluate` operation budget, honoring the
/// `TSZ_MAX_EVAL_OPS` override.
///
/// Defaults to [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`]. The override exists so tests
/// can force the cross-instance runaway bail at a small budget instead of
/// spinning through two million operations; a value of `0` (or an unparseable
/// value) falls back to the default.
pub(super) fn resolved_max_eval_ops() -> u32 {
    use std::sync::OnceLock;
    static OVERRIDE: OnceLock<Option<u32>> = OnceLock::new();
    let configured = *OVERRIDE.get_or_init(|| {
        std::env::var("TSZ_MAX_EVAL_OPS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|&v| v > 0)
    });
    configured.unwrap_or(DEFAULT_MAX_EVAL_OPS_PER_QUERY)
}

/// RAII frame that maintains the thread-local per-query `evaluate` operation
/// budget across every `TypeEvaluator` instance.
///
/// On construction it increments the live frame count (resetting the op counter
/// when starting a fresh top-level query) and bumps the op counter, recording
/// whether the budget is now exhausted. On drop it decrements the live frame
/// count, so the bound is restored on every return path, including panics.
pub(super) struct EvalQueryFrame {
    /// Whether the per-query operation budget was exhausted on entry.
    pub(super) budget_exhausted: bool,
}

impl EvalQueryFrame {
    #[inline]
    pub(super) fn enter(max_ops: u32) -> Self {
        let active = EVAL_QUERY_ACTIVE.with(|c| {
            let v = c.get();
            c.set(v + 1);
            v
        });
        if active == 0 {
            EVAL_QUERY_OPS.with(|c| c.set(0));
        }
        let ops = EVAL_QUERY_OPS.with(|c| {
            let v = c.get().saturating_add(1);
            c.set(v);
            v
        });
        Self {
            budget_exhausted: ops > max_ops,
        }
    }
}

impl Drop for EvalQueryFrame {
    #[inline]
    fn drop(&mut self) {
        EVAL_QUERY_ACTIVE.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_EVAL_OPS_PER_QUERY, EVAL_QUERY_ACTIVE, EVAL_QUERY_OPS, EvalQueryFrame,
        resolved_max_eval_ops,
    };
    use std::cell::Cell;

    /// The per-query operation counter resets when a fresh top-level query
    /// begins (live frame count returns to zero), so one type position can never
    /// carry its op count into the next.
    #[test]
    fn op_counter_resets_per_top_level_query() {
        {
            let _f1 = EvalQueryFrame::enter(1000);
            let _f2 = EvalQueryFrame::enter(1000);
            let _f3 = EvalQueryFrame::enter(1000);
            assert_eq!(EVAL_QUERY_OPS.with(Cell::get), 3);
            assert_eq!(EVAL_QUERY_ACTIVE.with(Cell::get), 3);
        }
        // All frames dropped -> live count back to zero.
        assert_eq!(EVAL_QUERY_ACTIVE.with(Cell::get), 0);

        // Second top-level query starts fresh: op counter reset to 1, not 4.
        let _f = EvalQueryFrame::enter(1000);
        assert_eq!(EVAL_QUERY_OPS.with(Cell::get), 1);
    }

    /// Once the budget is exceeded within a single query, the frame reports
    /// exhaustion so `evaluate` can bail; nested frames keep reporting it until
    /// the query unwinds.
    #[test]
    fn budget_exhaustion_is_reported_until_query_unwinds() {
        let f1 = EvalQueryFrame::enter(2);
        assert!(!f1.budget_exhausted, "op 1 of 2 is within budget");
        let f2 = EvalQueryFrame::enter(2);
        assert!(!f2.budget_exhausted, "op 2 of 2 is within budget");
        let f3 = EvalQueryFrame::enter(2);
        assert!(f3.budget_exhausted, "op 3 exceeds the budget of 2");
        let f4 = EvalQueryFrame::enter(2);
        assert!(
            f4.budget_exhausted,
            "still exhausted while the query is live"
        );
        drop((f1, f2, f3, f4));
        assert_eq!(EVAL_QUERY_ACTIVE.with(Cell::get), 0);
    }

    /// With no override set the resolved budget is the default.
    #[test]
    fn budget_defaults_when_env_unset() {
        assert_eq!(resolved_max_eval_ops(), DEFAULT_MAX_EVAL_OPS_PER_QUERY);
    }
}
