use crate::relations::subtype::{SubtypeChecker, TypeResolver};

use crate::types::{
    LiteralValue, ParamInfo, TemplateSpan, TupleElement, TypeApplication, TypeData, TypeId,
    TypeParamInfo,
};

use rustc_hash::{FxHashMap, FxHashSet};

use smallvec::SmallVec;

use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

use super::infer_substitutor::InferSubstitutor;

use std::cell::Cell;

/// Selects how co-located `infer T` candidates are merged when the same name
/// gets distinct bindings from multiple structurally adjacent positions.
#[derive(Clone, Copy)]
enum CoLocatedMerge {
    /// Tuple elements, array elements, optional-tail elements — covariant.
    Union,
    /// Function/callable parameters — contravariant.
    Intersection,
}

thread_local! {
    /// Cross-evaluator nesting depth for infer-pattern matching that expands an
    /// `Application`/`Mapped` source or pattern in a *fresh* sub-evaluator.
    ///
    /// Infer matching cannot call `evaluate` on the current `&self` evaluator
    /// (those methods take `&self`), so it spins up a brand-new `TypeEvaluator`
    /// whose per-instance recursion guard, depth counter, and fuel all start at
    /// zero. A recursive generic-wrapper application — Zod's
    /// `ZodObject`/`ZodOptional`/`ZodArray` chains, `DeepPartial`-style helpers,
    /// etc. — makes that expansion re-enter conditional/infer evaluation at a
    /// deeper nesting through a *new* evaluator each level, so no per-evaluator
    /// guard ever fires and the compile hangs. This thread-global counter bounds
    /// that cross-evaluator nesting. See [`TypeEvaluator::evaluate_for_infer_match`].
    static INFER_MATCH_EXPANSION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Maximum cross-evaluator nesting for infer-match sub-evaluator expansions.
///
/// Mirrors tsc's `instantiationDepth` cutoff (100): beyond this nesting, tsc
/// abandons the instantiation, so tsz stops expanding too rather than recurse
/// forever. Legitimate structural expansion of an application source during
/// infer matching never nests anywhere near this deep; only unbounded recursive
/// wrappers do.
const MAX_INFER_MATCH_EXPANSION_DEPTH: u32 = 100;

/// RAII guard for [`INFER_MATCH_EXPANSION_DEPTH`].
///
/// `enter` returns `None` when the budget is exhausted (the caller must then
/// skip the expansion); otherwise it increments the counter and decrements it
/// on drop, so the bound is restored even if evaluation unwinds via panic.
struct InferMatchExpansionGuard;

impl InferMatchExpansionGuard {
    fn enter() -> Option<Self> {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_INFER_MATCH_EXPANSION_DEPTH {
                None
            } else {
                depth.set(current + 1);
                Some(Self)
            }
        })
    }
}

impl Drop for InferMatchExpansionGuard {
    fn drop(&mut self) {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

include!("infer_pattern_parts/part1.rs");
include!("infer_pattern_parts/part2.rs");

#[cfg(test)]
mod infer_match_expansion_guard_tests {
    use super::{
        INFER_MATCH_EXPANSION_DEPTH, InferMatchExpansionGuard, MAX_INFER_MATCH_EXPANSION_DEPTH,
    };

    /// The cross-evaluator infer-match expansion guard must (1) allow expansion
    /// up to the budget, (2) deny it beyond the budget so the caller skips the
    /// expansion (returns the source unchanged) instead of recursing through a
    /// fresh evaluator forever, and (3) restore capacity as guards unwind, so
    /// sequential (non-nested) expansions are never throttled. This is the
    /// primitive that turns the Zod non-termination (#10662) into a terminating
    /// compile.
    #[test]
    fn guard_bounds_cross_evaluator_expansion_depth() {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(0));

        let mut held = Vec::new();
        for expected_prev in 0..MAX_INFER_MATCH_EXPANSION_DEPTH {
            let guard =
                InferMatchExpansionGuard::enter().expect("enter within budget must succeed");
            held.push(guard);
            assert_eq!(
                INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.get()),
                expected_prev + 1
            );
        }

        assert!(
            InferMatchExpansionGuard::enter().is_none(),
            "enter at the budget must be denied so the caller stops expanding"
        );

        held.clear();
        assert_eq!(INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.get()), 0);
        assert!(
            InferMatchExpansionGuard::enter().is_some(),
            "after unwinding, a fresh expansion must be allowed again"
        );
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(0));
    }
}
