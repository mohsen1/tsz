//! Per-rendered-type work budget for diagnostic display normalization.
//!
//! Rendering one diagnostic must be bounded by the size of the displayed
//! type — it must not re-run full type evaluation per node of an unbounded
//! expansion (issue #13040). Self-expanding generic applications (for
//! example `Awaited<...>` chains) intern fresh `TypeId`s on every
//! evaluation, so per-`TypeId` cycle sets and memos never converge. The
//! display normalization pass caps its recursion *depth*, but its *breadth*
//! is unbounded: each node fans out into freshly interned children and calls
//! `evaluate_type_for_assignability` again, which made diagnostic emission
//! effectively non-terminating on large recursive application types.
//!
//! A [`DisplayBudgetScope`] is entered at the top of each rendered-type
//! formatting entry point and grants a bounded amount of work:
//!
//! - a node-visit budget for the normalization tree
//!   ([`try_consume_visit`]), and
//! - an evaluation-fuel budget plus a result memo for
//!   `evaluate_type_for_assignability` ([`try_consume_eval_fuel`],
//!   [`cached_eval`], [`record_eval`]).
//!
//! When a budget is exhausted the caller returns the type unchanged — a hard
//! truncation, like tsc's `...` elision on long type displays. Outside an
//! active scope every helper is inert, so pure relation/semantic paths are
//! unaffected.
//!
//! The limits are deliberately far above what any realistically rendered
//! diagnostic consumes (the downstream formatter truncates nested printing
//! long before either budget is visible), so legitimate messages are
//! byte-identical; only pathological self-expanding normalization is cut
//! short.

use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use tsz_solver::TypeId;

/// Maximum normalization-tree node visits per rendered type.
const DISPLAY_VISIT_BUDGET: u32 = 50_000;

/// Maximum `evaluate_type_for_assignability` invocations per rendered type.
/// Nested self-recursive evaluation steps consume fuel too, so this bounds
/// the total evaluation work triggered by rendering one type.
const DISPLAY_EVAL_FUEL: u32 = 8_000;

struct DisplayBudget {
    visits: u32,
    eval_fuel: u32,
    eval_memo: FxHashMap<TypeId, TypeId>,
}

thread_local! {
    static ACTIVE: RefCell<Option<DisplayBudget>> = const { RefCell::new(None) };
    // Scope nesting depth, kept outside `ACTIVE` as a plain `Cell` so the
    // helpers below can answer "no scope active" — the common case on pure
    // relation/semantic paths — with a single cheap thread-local read
    // instead of a `RefCell` borrow.
    static SCOPE_DEPTH: Cell<u32> = const { Cell::new(0) };
}

fn scope_is_active() -> bool {
    SCOPE_DEPTH.with(|depth| depth.get() > 0)
}

/// RAII scope delimiting the rendering of one displayed type.
///
/// The outermost scope installs a fresh budget; nested scopes (rendering
/// helpers re-entering one another) share it. The budget is dropped when the
/// outermost scope exits.
pub(crate) struct DisplayBudgetScope;

impl DisplayBudgetScope {
    pub(crate) fn enter() -> Self {
        SCOPE_DEPTH.with(|depth| {
            if depth.get() == 0 {
                ACTIVE.with(|active| {
                    *active.borrow_mut() = Some(DisplayBudget {
                        visits: DISPLAY_VISIT_BUDGET,
                        eval_fuel: DISPLAY_EVAL_FUEL,
                        eval_memo: FxHashMap::default(),
                    });
                });
            }
            depth.set(depth.get() + 1);
        });
        Self
    }
}

impl Drop for DisplayBudgetScope {
    fn drop(&mut self) {
        SCOPE_DEPTH.with(|depth| {
            depth.set(depth.get().saturating_sub(1));
            if depth.get() == 0 {
                ACTIVE.with(|active| {
                    if let Some(budget) = active.borrow_mut().take()
                        && (budget.visits == 0 || budget.eval_fuel == 0)
                    {
                        tracing::debug!(
                            visits_left = budget.visits,
                            eval_fuel_left = budget.eval_fuel,
                            "display normalization budget exhausted; type rendered truncated"
                        );
                    }
                });
            }
        });
    }
}

fn try_consume(counter: fn(&mut DisplayBudget) -> &mut u32) -> bool {
    if !scope_is_active() {
        return true;
    }
    ACTIVE.with(|active| match active.borrow_mut().as_mut() {
        Some(budget) => {
            let counter = counter(budget);
            if *counter == 0 {
                false
            } else {
                *counter -= 1;
                true
            }
        }
        None => true,
    })
}

/// Consume one normalization node visit.
///
/// Returns `false` once the visit budget is exhausted; the caller must then
/// return the type unchanged instead of recursing. Always `true` when no
/// scope is active.
pub(crate) fn try_consume_visit() -> bool {
    try_consume(|budget| &mut budget.visits)
}

/// Consume one unit of evaluation fuel.
///
/// Returns `false` once the fuel is exhausted; the caller must then return
/// the input type unevaluated. Always `true` when no scope is active.
pub(crate) fn try_consume_eval_fuel() -> bool {
    try_consume(|budget| &mut budget.eval_fuel)
}

/// Look up a previously recorded evaluation result within the active scope.
pub(crate) fn cached_eval(type_id: TypeId) -> Option<TypeId> {
    if !scope_is_active() {
        return None;
    }
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .and_then(|budget| budget.eval_memo.get(&type_id).copied())
    })
}

/// Record a fully computed evaluation result within the active scope.
///
/// Only complete results may be recorded — cycle-truncated returns must not
/// be memoized, or later non-cyclic calls would observe the truncation.
pub(crate) fn record_eval(type_id: TypeId, result: TypeId) {
    if !scope_is_active() {
        return;
    }
    ACTIVE.with(|active| {
        if let Some(budget) = active.borrow_mut().as_mut() {
            budget.eval_memo.insert(type_id, result);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_without_scope() {
        for _ in 0..(DISPLAY_VISIT_BUDGET + DISPLAY_EVAL_FUEL) {
            assert!(try_consume_visit());
            assert!(try_consume_eval_fuel());
        }
        assert_eq!(cached_eval(TypeId::STRING), None);
        record_eval(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(cached_eval(TypeId::STRING), None);
    }

    #[test]
    fn visit_budget_exhausts_and_resets_per_scope() {
        {
            let _scope = DisplayBudgetScope::enter();
            for _ in 0..DISPLAY_VISIT_BUDGET {
                assert!(try_consume_visit());
            }
            assert!(!try_consume_visit());
            assert!(!try_consume_visit());
        }
        let _scope = DisplayBudgetScope::enter();
        assert!(try_consume_visit());
    }

    #[test]
    fn eval_fuel_exhausts_and_resets_per_scope() {
        {
            let _scope = DisplayBudgetScope::enter();
            for _ in 0..DISPLAY_EVAL_FUEL {
                assert!(try_consume_eval_fuel());
            }
            assert!(!try_consume_eval_fuel());
        }
        let _scope = DisplayBudgetScope::enter();
        assert!(try_consume_eval_fuel());
    }

    #[test]
    fn nested_scopes_share_one_budget() {
        let _outer = DisplayBudgetScope::enter();
        assert!(try_consume_visit());
        {
            let _inner = DisplayBudgetScope::enter();
            for _ in 0..(DISPLAY_VISIT_BUDGET - 1) {
                assert!(try_consume_visit());
            }
            assert!(!try_consume_visit());
        }
        // Inner scope exit must not reset the outer scope's budget.
        assert!(!try_consume_visit());
    }

    #[test]
    fn eval_memo_is_scoped() {
        {
            let _scope = DisplayBudgetScope::enter();
            assert_eq!(cached_eval(TypeId::STRING), None);
            record_eval(TypeId::STRING, TypeId::NUMBER);
            assert_eq!(cached_eval(TypeId::STRING), Some(TypeId::NUMBER));
        }
        let _scope = DisplayBudgetScope::enter();
        assert_eq!(cached_eval(TypeId::STRING), None);
    }
}
