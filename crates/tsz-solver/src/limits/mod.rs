//! Consolidated depth/fuel limit policy and thread-local budget state for the
//! solver's evaluation / instantiation / relation stack (issue #13091).
//!
//! # Why this module exists
//!
//! The recursive solver operations are bounded by a family of *named,
//! per-operation-class* limits, exactly as `tsc`'s `checker.ts` runs distinct
//! `instantiationDepth` / `instantiationCount` / relation-depth /
//! `relationCount` / tail-recursion budgets. The limits are deliberately NOT
//! unified into one depth+fuel pair: each class has different firing
//! semantics (hard `TS2589` error vs. silent opaque bail vs. assumed-related
//! `Ternary.Maybe`), and this policy area is regression-prone in both
//! directions (a depth-bail policy change once caused a 7.1x `ts-toolbelt`
//! slowdown, #6973 → recovered in #7210).
//!
//! What WAS consolidated here is the *mechanism*:
//! - every limit constant is defined (or re-exported) in this one module, so
//!   the whole policy surface is auditable at a glance;
//! - the scattered per-counter `thread_local!` cells are merged into one
//!   [`LimitBudgets`] struct behind a single `thread_local!`, so hot paths
//!   that touch several counters per frame resolve TLS once (macOS
//!   `__tls_get_addr` is ~10-15ns per access; see the same optimization note
//!   in `intern/core/interner/cache.rs`);
//! - the limit-hit result-cache policy from issue #13241 (`maybeKeys`
//!   promotion kill switch, fuel-band honesty helper) lives here so any
//!   future per-class `Maybe` promotion is wired in one place.
//!
//! Thresholds are intentionally byte-for-byte identical to their previous
//! scattered definitions: behavior preservation is the acceptance gate.
//!
//! # Guard inventory
//!
//! | Guard (owner) | Limit | tsc analogue | Firing semantics | Witness |
//! |---|---|---|---|---|
//! | `SubtypeChecker.guard` per-instance `RecursionGuard<(TypeId,TypeId)>` (`relations/subtype/core.rs`) | [`MAX_SUBTYPE_DEPTH`] = 100 depth, 100k iterations (`RecursionProfile::SubtypeCheck`) | `recursiveTypeRelatedTo` source/target depth 100 → `Ternary.Maybe`; `relationCount` 1M → TS2859 | `DepthExceeded` is assumed-related (`is_true`), uncached unless promoted via the maybe stack | `limit_relation_cache_tests`, recursive interface conformance |
//! | `SubtypeChecker.def_guard` per-instance `RecursionGuard<(DefId,DefId)>` (`relations/subtype/cache.rs`) | same profile | `getRecursionIdentity` cycle keys | `Cycle` → coinductive assumed-related | recursive generic interface tests |
//! | Global subtype chain fuel, thread-local (this module) | [`MAX_GLOBAL_SUBTYPE_FUEL`] = 10k non-trivial checks per top-level chain | no direct analogue (closest: `relationCount`) | assumed-related `DepthExceeded`, cacheable only as fuel-band `LimitTrue` (#13241) | `limit_relation_cache_tests::fuel_*` |
//! | `TypeEvaluator.guard` per-instance `RecursionGuard<TypeId>` (`evaluation/evaluate.rs`) | depth 100, 100k iterations (`RecursionProfile::TypeEvaluation`) | structural stack protection (no direct tsc analogue) | depth: escalate to `TS2589` only when [`REAL_INSTANTIATION_BAILOUT_THRESHOLD`] also holds, else silent opaque bail | type-challenges `Permutation` family |
//! | `TypeEvaluator.def_depth` per-`DefId` map (`evaluation/evaluate.rs`) | [`MAX_DEF_DEPTH`] = 100, escalation floor [`REAL_INSTANTIATION_BAILOUT_THRESHOLD`] = 40 | `instantiationDepth` (100) → TS2589 | hard bail, memoized `ERROR` when real | TS2589 conformance, `TrimRight` aliases |
//! | Conditional tail-recursion loop (`evaluation/evaluate_rules/conditional.rs`) | [`MAX_TAIL_RECURSION_DEPTH`] = 1000 | `getConditionalType` `tailCount` 1000 (exact parity) | `TS2589` + `ERROR` | tail-recursive conditional tests |
//! | Cross-evaluator stack depth, thread-local (this module) | [`MAX_GLOBAL_EVAL_DEPTH`] = 200 live frames | none (fresh-instance artifact) | silent opaque bail (`mark_silent_depth_bailed`) | deep `implements` chains |
//! | Conditional subtype relation depth, `EvaluationSession` (`evaluation/session.rs`) | [`MAX_CONDITIONAL_SUBTYPE_DEPTH`] = 50 re-entrant branch probes | `instantiationDepth` adjacent | conservative false, not published to branch cache | recursive conditional subtype probes |
//! | Infer-match fresh-evaluator expansion depth, `EvaluationSession` (`evaluation/session.rs`) | [`MAX_INFER_MATCH_EXPANSION_DEPTH`] = 100 nested expansions | `instantiationDepth` | leave source/pattern opaque and skip infer expansion | recursive conditional/`infer` wrappers |
//! | Checker lazy-resolution fuel, `EvaluationSession` (`evaluation/session.rs`) | [`MAX_CHECKER_LAZY_RESOLUTION_FUEL`] = 50k forced lazy/enum/type-query resolutions | no direct analogue (checker readiness artifact) | stop publishing degraded memo/failure results and leave remaining refs lazy | `lazy_lib_fuel_determinism`, DOM fuel exhaustion tests |
//! | Checker lazy-readiness guards, `EvaluationSession` (`evaluation/session.rs`) | [`MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH`] = 1, [`MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL`] = 200, [`MAX_CHECKER_REFS_RESOLUTION_FUEL`] = 2000, [`MAX_CHECKER_EVAL_ENV_DEPTH`] = 5 | no direct analogue (checker eager-readiness artifact) | stop the local readiness prewalk or leave env-eval root opaque | `lazy_guard_state`, `lazy_resolution_session_scans` |
//! | Per-query eval op budget, thread-local (this module) | [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`] = 2M ops per top-level query (`TSZ_MAX_EVAL_OPS` override) | `instantiationCount` (5M) per checked element | silent opaque bail | `Unbox`/`Awaited` runaway tests (`query_budget`) |
//! | Per-file evaluation fuel, thread-local (this module) | [`MAX_EVALUATION_FUEL`] = 2M, sampled every [`EVAL_FUEL_CHECK_INTERVAL`] = 128 guard iterations | `instantiationCount` (5M; tsz lower because eager expansion is heavier) | `TS2589`-style `ERROR` bail | ts-toolbelt corpus, #13172/#13181 |
//! | `TypeInstantiator.depth` per-instance (`instantiation/instantiate.rs`) | [`MAX_TYPE_SUBSTITUTION_DEPTH`] = 50 | `instantiateType` recursion (tsc `instantiationDepth` = 100; see note below) | sticky `depth_exceeded`, returns input type opaque | recursive generic instantiation tests |
//! | `EvaluationSession` Rc-shared cross-context counters (`evaluation/session.rs`) | [`MAX_GLOBAL_INSTANTIATION_DEPTH`] = 50, [`MAX_GLOBAL_INSTANTIATION_FUEL`] = 2000 per file | `instantiationDepth`/`instantiationCount` at the checker boundary | checker leaves application un-expanded | react16.d.ts corpus |
//! | Cross-operation stack-frame breaker, thread-local (this module, RAII in `recursion.rs`) | [`MAX_SOLVER_STACK_FRAMES`] = 2000 live frames | none (OS-stack protection, issue #7574) | relation-preserving default (assumed-related / opaque) | #7574 10k-file repo overflow; xstate canary |
//!
//! # Known double-fire / divergence findings (documented, NOT changed here)
//!
//! - `tsz_common::limits::MAX_INSTANTIATION_DEPTH` (100, checker-side TS2589,
//!   tsc parity) vs the solver instantiator's [`MAX_TYPE_SUBSTITUTION_DEPTH`]
//!   (50) carried the *same name* in two crates with different values. The
//!   solver constant is renamed here to make the divergence visible; the
//!   public `instantiation::instantiate::MAX_INSTANTIATION_DEPTH` alias keeps
//!   its value (50). Aligning to tsc's 100 would be a behavior change
//!   requiring its own witness.
//! - [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`] and [`MAX_EVALUATION_FUEL`] both
//!   mirror tsc's `instantiationCount` and both count evaluator work (every
//!   `evaluate` op per query vs. guard iterations per file, sampled at
//!   [`EVAL_FUEL_CHECK_INTERVAL`]). They fire for different runaway shapes
//!   (cross-instance bounce vs. cumulative file budget) and cannot be merged
//!   without changing which witness bails first.
//! - The same recursive descent is depth-counted by up to three stack guards
//!   (per-instance guard depth 100, cross-evaluator depth 200, shared solver
//!   frames 2000) at different scopes. Their counter *updates* were partially
//!   deduplicated (single-TLS frame entry), but all three limits are kept:
//!   each catches a recursion shape the others structurally cannot.
//! - The evaluator's per-`TypeId` structural depth (100) and per-`DefId`
//!   expansion depth ([`MAX_DEF_DEPTH`] = 100) count the same conditional
//!   descent; which fires first decides silent-opaque vs. TS2589. The
//!   [`REAL_INSTANTIATION_BAILOUT_THRESHOLD`] = 40 escalation floor is the
//!   deliberate tie-breaker (calibration notes at the constant).

use std::cell::Cell;

// =============================================================================
// Named per-class limits (policy). Values are unchanged from their previous
// scattered definitions; do not retune without a dedicated witness test and
// tsc evidence (see module doc).
// =============================================================================

/// Maximum recursion depth for structural subtype checking
/// (`SubtypeChecker::max_depth`). Re-exported from `tsz_common` because the
/// checker shares it. tsc: `recursiveTypeRelatedTo` depth 100.
pub(crate) const MAX_SUBTYPE_DEPTH: u32 = tsz_common::limits::MAX_SUBTYPE_DEPTH;

/// Maximum number of non-trivial subtype checks per top-level relation chain
/// (cross-instance, thread-local). Generous enough for complex real-world
/// types (react, fp-ts) but restrictive enough to prevent runaway recursion
/// from hanging. Exhaustion returns assumed-related `DepthExceeded`
/// (tsc `Ternary.Maybe` semantics).
pub(crate) const MAX_GLOBAL_SUBTYPE_FUEL: u32 = 10_000;

/// Maximum recursive expansion depth for a single `DefId` in the evaluator.
/// Matches TypeScript's `instantiationDepth` limit that triggers TS2589.
pub(crate) const MAX_DEF_DEPTH: u32 = 100;

/// When the structural per-`TypeId` recursion guard hits its depth limit,
/// surface it as TS2589 only if some `DefId` has been recursively expanded at
/// least this many times — otherwise treat the bailout as the
/// stack-protection cost of legitimate finite recursion and leave the type
/// opaque.
///
/// Calibration: empirically, `Permutation<U>` with `|U| ≤ 3` peaks around
/// `def_depth ≈ 33` when it hits the structural limit, while unbounded
/// patterns like `type Foo<T,B> = { "true": Foo<T, Foo<T,B>> }[T]` saturate
/// near `def_depth ≈ 50`.
pub(crate) const REAL_INSTANTIATION_BAILOUT_THRESHOLD: u32 = 40;

/// Maximum depth for tail-recursive conditional evaluation. Allows patterns
/// like `type Loop<T> = T extends [...infer R] ? Loop<R> : never` to work
/// with up to 1000 recursive calls. Exact parity with tsc's `tailCount`
/// limit in `getConditionalType`.
pub(crate) const MAX_TAIL_RECURSION_DEPTH: usize = 1000;

/// Maximum depth for recursive type *substitution* in `TypeInstantiator`.
///
/// NOTE: this is half of tsc's `instantiationDepth` (100) and of the
/// checker-side `tsz_common::limits::MAX_INSTANTIATION_DEPTH` (100); it
/// bounds the structural substitution walk, not the instantiation stack.
/// Named distinctly so the divergence stays visible (see module doc).
pub(crate) const MAX_TYPE_SUBSTITUTION_DEPTH: u32 = 50;

/// Maximum distinct type nodes one `InferSubstitutor` traversal may visit
/// (breadth bound for infer-binding substitution).
///
/// `recursion::with_solver_frame` (see [`MAX_SOLVER_STACK_FRAMES`]) bounds how
/// *deep* a single substitution recurses, but it is RAII-balanced, so a type
/// that is shallow yet enormously *wide* — or one whose conditional expansion
/// interns fresh `TypeId`s on every level, the self-expanding family in issue
/// #13040 — keeps the live frame count low and never trips the depth guard.
/// One such substitution then walks an unbounded number of distinct nodes,
/// which is a (non-depth) channel of the diagnostic display/eval non-termination
/// that the per-rendered-type display budget bounds on the checker side.
///
/// This cap bounds total node visits per traversal. On exhaustion the
/// substitutor leaves the remaining nodes opaque (identity), the same
/// relation-preserving bail the depth guard already takes via
/// `with_solver_frame(...).unwrap_or(type_id)`. It is calibrated far above any
/// legitimately substituted type (a successfully-checked 1.47 M LOC corpus
/// visits orders of magnitude fewer nodes in any single substitution), so a
/// terminating substitution is byte-identical; only a runaway is truncated.
pub(crate) const MAX_INFER_SUBSTITUTION_NODES: u32 = 1_000_000;

/// Maximum cumulative evaluation fuel across all `TypeEvaluator` instances
/// of one file-check session (thread-local, reset per file).
///
/// Mirrors TypeScript's `instantiationCount` limit (5,000,000 in tsc). Set
/// lower than tsc's limit because tsz's per-evaluation work is heavier (tsz
/// eagerly expands where tsc defers). When exceeded, evaluators return
/// `TypeId::ERROR`, matching TS2589.
pub(crate) const MAX_EVALUATION_FUEL: u32 = 2_000_000;

/// Interval (in per-instance guard iterations) for sampling the per-file
/// evaluation fuel counter, amortizing the TLS access on the hot path.
pub(crate) const EVAL_FUEL_CHECK_INTERVAL: u32 = 128;

/// Maximum live `evaluate` stack frames summed across every `TypeEvaluator`
/// instance on the thread (cross-evaluator stack overflow prevention).
pub(crate) const MAX_GLOBAL_EVAL_DEPTH: u32 = 200;

/// Maximum re-entrant conditional-subtype branch probes in one evaluation
/// session. Exhaustion conservatively treats the relation as false and marks
/// the result budget-bounded so it is not published to the branch cache.
pub(crate) const MAX_CONDITIONAL_SUBTYPE_DEPTH: u32 = 50;

/// Maximum cross-evaluator nesting for infer-match sub-evaluator expansion.
/// Mirrors tsc's `instantiationDepth` cutoff: beyond this nesting, tsz leaves
/// the source or pattern unchanged instead of recursing through another fresh
/// evaluator with empty per-instance guards.
pub(crate) const MAX_INFER_MATCH_EXPANSION_DEPTH: u32 = 100;

/// Maximum checker lazy-resolution fuel across all top-level calls in one
/// shared evaluation session.
pub(crate) const MAX_CHECKER_LAZY_RESOLUTION_FUEL: u32 = 50_000;

/// Maximum nested `ensure_application_symbols_resolved` calls.
pub(crate) const MAX_CHECKER_APP_SYMBOL_RESOLUTION_DEPTH: u32 = 1;

/// Maximum `DefId`/type-query resolutions within
/// `ensure_application_symbols_resolved`.
pub(crate) const MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL: u32 = 200;

/// Maximum `DefId` resolutions across recursive `ensure_refs_resolved`
/// cascades within one top-level call.
pub(crate) const MAX_CHECKER_REFS_RESOLUTION_FUEL: u32 = 2000;

/// Maximum recursive `evaluate_type_with_env_impl` depth.
pub(crate) const MAX_CHECKER_EVAL_ENV_DEPTH: u32 = 5;

/// Total `evaluate` operations permitted for a single top-level evaluation
/// query (the outermost `evaluate` call on the thread, before it returns).
///
/// Defaults to the whole-file [`MAX_EVALUATION_FUEL`]: a single top-level
/// query that out-works the entire file's evaluation-fuel budget is, by
/// construction, a runaway. Overridable via `TSZ_MAX_EVAL_OPS` (see
/// `evaluation::evaluate::query_budget::resolved_max_eval_ops`).
pub(crate) const DEFAULT_MAX_EVAL_OPS_PER_QUERY: u32 = 2_000_000;

/// Maximum simultaneously active solver recursion frames on a single thread,
/// summed across every recursive solver operation (cross-operation
/// stack-frame breaker; calibration rationale in `recursion.rs`).
pub(crate) const MAX_SOLVER_STACK_FRAMES: u32 = 2_000;

/// Maximum global instantiation depth for `EvaluationSession` — bounds
/// nesting of `evaluate_application_type` calls across all `CheckerContext`
/// instances.
pub(crate) const MAX_GLOBAL_INSTANTIATION_DEPTH: u32 = 50;

/// Maximum global instantiation fuel for `EvaluationSession` — limits TOTAL
/// non-cached `evaluate_application_type` invocations per file. React's
/// react16.d.ts can trigger thousands of unique Application evaluations;
/// this caps work.
pub(crate) const MAX_GLOBAL_INSTANTIATION_FUEL: u32 = 2000;

// =============================================================================
// Consolidated thread-local budget state (mechanism).
// =============================================================================

/// All thread-local depth/fuel counters of the solver's recursive operation
/// classes, merged behind one `thread_local!` so multi-counter hot paths pay
/// a single TLS address resolution per frame event.
///
/// Each field keeps the exact update arithmetic of the scattered cell it
/// replaced; accessors below are the only mutation points.
struct LimitBudgets {
    /// Packed global subtype chain state: high 32 bits = fuel consumed,
    /// low 32 bits = live chain depth. Fuel is monotonically consumed and
    /// reset only when the outermost `check_subtype` of the chain exits.
    subtype_state: Cell<u64>,
    /// Monotonic counter bumped whenever a `Lazy(DefId)` could not be
    /// resolved (its body is not yet registered — typically a re-entrant
    /// lib-resolution window).
    ///
    /// This remains a public compatibility sentinel for checker-side proof
    /// caches and conditional branch probes that run outside the relation frame
    /// owner. The relation cache itself uses a `SubtypeChecker`-local
    /// unresolved-lazy counter so fresh/nested checker probes do not suppress
    /// unrelated relation cache writes.
    lazy_resolve_failures: Cell<u64>,
    /// Monotonic counter bumped whenever a structural comparison hits the
    /// weak-type (TS2559) trigger: a non-empty, non-weak source compared
    /// against a weak-type target with no common property names. That
    /// enforcement state is operation-local and NOT encoded in the
    /// flag-agnostic `RelationCacheKey`, so a result computed while this
    /// counter changed must not be memoized in the shared relation cache.
    weak_type_sensitivity: Cell<u64>,
    /// Live count of nested `evaluate` frames across *all* `TypeEvaluator`
    /// instances on the current thread. `0` means no evaluation is in
    /// flight, so the next `evaluate` begins a fresh top-level query.
    eval_query_active: Cell<u32>,
    /// Total `evaluate` operations performed in the current top-level query.
    /// Reset whenever `eval_query_active` transitions from `0`.
    eval_query_ops: Cell<u32>,
    /// Live cross-evaluator `evaluate` stack depth (only frames whose
    /// per-instance guard depth is already significant; see
    /// `TypeEvaluator::evaluate`).
    global_eval_depth: Cell<u32>,
    /// Per-file cumulative evaluation fuel (see [`MAX_EVALUATION_FUEL`]).
    evaluation_fuel: Cell<u32>,
    /// Live cross-operation solver recursion frames (see
    /// [`MAX_SOLVER_STACK_FRAMES`] and `recursion::SolverStackFrame`).
    solver_stack_frames: Cell<u32>,
    /// Monotonic count of solver-frame-budget curtailments on this thread.
    /// Bumped each time [`solver_stack_frame_try_enter`] denies a frame because
    /// [`MAX_SOLVER_STACK_FRAMES`] is already active. A *nested* `evaluate_*`
    /// curtailed this way returns an under-evaluated form WITHOUT setting the
    /// outer instantiator's `depth_exceeded` (the instantiator's own frame is
    /// already on the stack), so the project-wide instantiation cache snapshots
    /// this counter before/after a walk and refuses to store a result whose
    /// computation saw a curtailment — the instantiation-layer analog of
    /// `closed_eval`'s per-node `tainted` limit exclusion.
    solver_frame_bail_count: Cell<u32>,
}

impl LimitBudgets {
    const fn new() -> Self {
        Self {
            subtype_state: Cell::new(0),
            lazy_resolve_failures: Cell::new(0),
            weak_type_sensitivity: Cell::new(0),
            eval_query_active: Cell::new(0),
            eval_query_ops: Cell::new(0),
            global_eval_depth: Cell::new(0),
            evaluation_fuel: Cell::new(0),
            solver_stack_frames: Cell::new(0),
            solver_frame_bail_count: Cell::new(0),
        }
    }

    /// Zero every transient budget/guard counter back to its fresh-process
    /// value. All fields are per-compilation scratch state (chain depth/fuel,
    /// per-query op budget, cross-evaluator depth, per-file evaluation fuel,
    /// solver recursion frames, and the two cache-poisoning sentinels); none
    /// of them carry meaning across a compilation boundary. New fields added
    /// to [`LimitBudgets`] MUST be reset here so batch/row reuse stays
    /// schedule-independent (see [`reset_subtype_thread_local_state`]).
    fn reset_all(&self) {
        self.subtype_state.set(0);
        self.lazy_resolve_failures.set(0);
        self.weak_type_sensitivity.set(0);
        self.eval_query_active.set(0);
        self.eval_query_ops.set(0);
        self.global_eval_depth.set(0);
        self.evaluation_fuel.set(0);
        self.solver_stack_frames.set(0);
        self.solver_frame_bail_count.set(0);
    }
}

thread_local! {
    static LIMIT_BUDGETS: LimitBudgets = const { LimitBudgets::new() };
}

/// Pack subtype chain depth (low 32) and fuel (high 32) into a single u64.
#[inline(always)]
const fn pack_depth_fuel(depth: u32, fuel: u32) -> u64 {
    (fuel as u64) << 32 | depth as u64
}

/// Extract subtype chain depth from packed state.
#[inline(always)]
const fn unpack_depth(state: u64) -> u32 {
    state as u32
}

/// Extract subtype chain fuel from packed state.
#[inline(always)]
const fn unpack_fuel(state: u64) -> u32 {
    (state >> 32) as u32
}

// -----------------------------------------------------------------------------
// Subtype relation chain (global fuel/depth + cache-poisoning sentinels)
// -----------------------------------------------------------------------------

/// Snapshot returned by [`enter_subtype_frame`]: the chain state *before*
/// this frame's increment plus the weak-type cache-poisoning sentinel and the
/// shared solver-frame depth, all read under one TLS resolution.
#[derive(Copy, Clone, Debug)]
pub(crate) struct SubtypeFrameEntry {
    /// Chain depth before this frame entered (0 = this frame is outermost).
    pub(crate) global_depth: u32,
    /// Fuel consumed by the chain before this frame entered.
    pub(crate) fuel: u32,
    /// [`weak_type_sensitivity_count`] at entry.
    pub(crate) weak_sensitivity: u64,
    /// [`solver_stack_frame_depth`](crate::recursion::solver_stack_frame_depth)
    /// at entry (read for the pristine-budget-chain promotion gate, #13241).
    pub(crate) solver_stack_frames: u32,
}

/// Enter one non-trivial `check_subtype` frame: increments chain depth and
/// consumes one unit of global fuel, returning the pre-increment state and
/// weak-type sentinel snapshot. Single TLS access.
#[inline]
pub(crate) fn enter_subtype_frame() -> SubtypeFrameEntry {
    LIMIT_BUDGETS.with(|b| {
        let prev = b.subtype_state.get();
        let depth = unpack_depth(prev);
        let fuel = unpack_fuel(prev);
        b.subtype_state.set(pack_depth_fuel(depth + 1, fuel + 1));
        SubtypeFrameEntry {
            global_depth: depth,
            fuel,
            weak_sensitivity: b.weak_type_sensitivity.get(),
            solver_stack_frames: b.solver_stack_frames.get(),
        }
    })
}

/// Leave one `check_subtype` frame: decrements chain depth, and when the
/// outermost frame of the chain exits (`was_outermost`), resets the chain
/// fuel for the next top-level relation.
#[inline]
pub(crate) fn leave_subtype_frame(was_outermost: bool) {
    LIMIT_BUDGETS.with(|b| {
        let prev = b.subtype_state.get();
        let depth = unpack_depth(prev).saturating_sub(1);
        if was_outermost {
            b.subtype_state.set(pack_depth_fuel(depth, 0));
        } else {
            b.subtype_state
                .set(pack_depth_fuel(depth, unpack_fuel(prev)));
        }
    })
}

/// Remaining global subtype fuel budget for the current thread's in-flight
/// relation chain. [`MAX_GLOBAL_SUBTYPE_FUEL`] when no chain is in flight.
///
/// Used to decide whether a budget-conditional
/// `RelationCacheValue::LimitTrue` entry is honest for the current query:
/// the recorded verdict only holds for queries whose remaining budget is at
/// most the entry's `fuel_band` (a larger budget could complete the
/// comparison honestly and must recompute).
#[inline]
pub(crate) fn remaining_global_subtype_fuel() -> u32 {
    LIMIT_BUDGETS
        .with(|b| MAX_GLOBAL_SUBTYPE_FUEL.saturating_sub(unpack_fuel(b.subtype_state.get())))
}

/// Record that a `Lazy(DefId)` failed to resolve during a relation check.
#[inline]
pub(crate) fn note_lazy_resolve_failure() {
    LIMIT_BUDGETS.with(|b| {
        b.lazy_resolve_failures
            .set(b.lazy_resolve_failures.get().wrapping_add(1));
    });
}

/// Current value of the unresolved-`Lazy` counter; compare a snapshot taken
/// before computing a result with the value after to detect whether the
/// computation depended on an unresolved `Lazy`.
///
/// Public so checker-side proof caches shared across file checkers can apply
/// the same "don't publish results that depended on an unresolved `Lazy`"
/// suppression the solver's shared relation cache uses.
#[inline]
pub fn lazy_resolve_failure_count() -> u64 {
    LIMIT_BUDGETS.with(|b| b.lazy_resolve_failures.get())
}

/// Record that a structural comparison reached the weak-type (TS2559)
/// trigger, making the in-flight result sensitive to the active weak-type
/// enforcement state. See [`LimitBudgets::weak_type_sensitivity`].
#[inline]
pub(crate) fn note_weak_type_sensitivity() {
    LIMIT_BUDGETS.with(|b| {
        b.weak_type_sensitivity
            .set(b.weak_type_sensitivity.get().wrapping_add(1));
    });
}

/// Current value of the weak-type sensitivity counter; compare a snapshot taken
/// before computing a relation result with the value after to detect whether
/// the computation depended on weak-type enforcement state that the
/// flag-agnostic relation cache key does not encode.
#[inline]
pub(crate) fn weak_type_sensitivity_count() -> u64 {
    LIMIT_BUDGETS.with(|b| b.weak_type_sensitivity.get())
}

/// Both legacy cache-poisoning sentinel counters under one TLS resolution:
/// `(lazy_resolve_failures, weak_type_sensitivity)`. Checker-side proof caches
/// still snapshot both counters; relation cache writes now use a
/// `SubtypeChecker`-local unresolved-lazy counter plus the weak-type sentinel.
#[cfg(test)]
#[inline]
pub(crate) fn poison_sentinel_counts() -> (u64, u64) {
    LIMIT_BUDGETS.with(|b| (b.lazy_resolve_failures.get(), b.weak_type_sensitivity.get()))
}

/// Reset every transient thread-local limit budget to its fresh-process value.
///
/// Called between compilation sessions (batch/row reuse) to prevent stale
/// state from a previous compilation leaking into the next on a reused worker
/// thread. Originally this reset only the subtype chain depth/fuel and the two
/// cache-poisoning sentinels, but [`LimitBudgets`] also carries the per-query
/// op budget, the cross-evaluator eval depth, the per-file evaluation fuel, and
/// the solver recursion-frame count. Some of those use manual (non-RAII)
/// enter/leave, so a compilation that bailed via the stack/depth breaker or a
/// caught-and-swallowed panic could leave them non-zero — making the next
/// compilation's depth/fuel/op verdicts (and the eval-query-gated
/// `reset_query_memo`, which only clears when the op-budget frame count
/// transitions from zero) schedule-dependent (#13368). Resetting the whole
/// struct keeps batch results byte-identical to a fresh process regardless of
/// what ran before on the thread.
pub fn reset_subtype_thread_local_state() {
    LIMIT_BUDGETS.with(LimitBudgets::reset_all);
}

// -----------------------------------------------------------------------------
// Evaluator per-query operation budget
// -----------------------------------------------------------------------------

/// Result of [`eval_query_enter`]: whether this frame began a fresh
/// top-level query, and the op count *after* this frame's bump.
#[derive(Copy, Clone, Debug)]
pub(crate) struct EvalQueryEntry {
    /// True when no `evaluate` frame was in flight on this thread, i.e. this
    /// frame starts a fresh top-level query (the op counter was reset before
    /// the bump).
    pub(crate) began_top_level_query: bool,
    /// Op count for the current top-level query, including this frame.
    pub(crate) ops: u32,
}

/// Enter one `evaluate` frame of the cross-instance per-query budget:
/// increments the live frame count, resets the op counter when a fresh
/// top-level query begins, and bumps the op counter. Single TLS access
/// (previously three separate `thread_local!` operations).
#[inline]
pub(crate) fn eval_query_enter() -> EvalQueryEntry {
    LIMIT_BUDGETS.with(|b| {
        let active = b.eval_query_active.get();
        b.eval_query_active.set(active + 1);
        if active == 0 {
            b.eval_query_ops.set(0);
        }
        let ops = b.eval_query_ops.get().saturating_add(1);
        b.eval_query_ops.set(ops);
        EvalQueryEntry {
            began_top_level_query: active == 0,
            ops,
        }
    })
}

/// Leave one `evaluate` frame of the per-query budget (RAII-called from
/// `EvalQueryFrame::drop`, including during panic unwinds).
#[inline]
pub(crate) fn eval_query_leave() {
    LIMIT_BUDGETS.with(|b| {
        b.eval_query_active
            .set(b.eval_query_active.get().saturating_sub(1));
    });
}

/// Live `evaluate` frame count (test/diagnostic accessor).
#[cfg(test)]
pub(crate) fn eval_query_active() -> u32 {
    LIMIT_BUDGETS.with(|b| b.eval_query_active.get())
}

/// Current top-level-query op count (test/diagnostic accessor).
#[cfg(test)]
pub(crate) fn eval_query_ops() -> u32 {
    LIMIT_BUDGETS.with(|b| b.eval_query_ops.get())
}

// -----------------------------------------------------------------------------
// Cross-evaluator stack depth
// -----------------------------------------------------------------------------

/// Enter one cross-evaluator `evaluate` stack frame: increments the live
/// depth and returns the pre-increment value for comparison against
/// [`MAX_GLOBAL_EVAL_DEPTH`].
#[inline]
pub(crate) fn global_eval_depth_enter() -> u32 {
    LIMIT_BUDGETS.with(|b| {
        let v = b.global_eval_depth.get();
        b.global_eval_depth.set(v + 1);
        v
    })
}

/// Leave one cross-evaluator `evaluate` stack frame.
#[inline]
pub(crate) fn global_eval_depth_leave() {
    LIMIT_BUDGETS.with(|b| {
        b.global_eval_depth
            .set(b.global_eval_depth.get().saturating_sub(1));
    });
}

// -----------------------------------------------------------------------------
// Per-file evaluation fuel
// -----------------------------------------------------------------------------

/// Consume `amount` units of this thread's per-file evaluation fuel and
/// return whether the budget is now exhausted (see [`MAX_EVALUATION_FUEL`]).
///
/// Thread-local rather than process-global on purpose: each file-check
/// session runs entirely on one worker thread and resets the budget at
/// session start. A process-global counter made concurrent fresh-checker
/// workers consume and reset each other's budget, so whether a deep
/// evaluation bailed to `TypeId::ERROR` depended on sibling-worker
/// scheduling — the source of flaky parallel-check diagnostics (false
/// TS2344) and uncached re-evaluation storms on type-heavy projects
/// (#13172/#13181).
#[inline]
pub(crate) fn consume_evaluation_fuel(amount: u32) -> bool {
    LIMIT_BUDGETS.with(|b| {
        let next = b.evaluation_fuel.get().wrapping_add(amount);
        b.evaluation_fuel.set(next);
        next > MAX_EVALUATION_FUEL
    })
}

/// Reset this thread's evaluation fuel counter.
///
/// Called at the start of each top-level file check session. `tsc` resets
/// its `instantiationCount` per checked source element, so the fuel limit
/// must bound *per-check* runaway instantiation rather than accumulate
/// across the whole program — a cumulative budget starves the tail files of
/// any multi-thousand-file program into blanket `TypeId::ERROR`.
#[inline]
pub(crate) fn reset_evaluation_fuel() {
    LIMIT_BUDGETS.with(|b| b.evaluation_fuel.set(0));
}

/// Check whether this thread's evaluation fuel is exhausted without
/// consuming any.
#[inline]
pub(crate) fn is_evaluation_fuel_exhausted() -> bool {
    LIMIT_BUDGETS.with(|b| b.evaluation_fuel.get() > MAX_EVALUATION_FUEL)
}

// -----------------------------------------------------------------------------
// Cross-operation solver stack frames
// -----------------------------------------------------------------------------

/// Try to account one cross-operation solver recursion frame. Returns `true`
/// (frame entered, caller owes a matching [`solver_stack_frame_leave`]) when
/// the budget has headroom, `false` when [`MAX_SOLVER_STACK_FRAMES`] frames
/// are already active. RAII wrapper: `recursion::SolverStackFrame`.
#[inline]
pub(crate) fn solver_stack_frame_try_enter() -> bool {
    LIMIT_BUDGETS.with(|b| {
        let depth = b.solver_stack_frames.get();
        if depth >= MAX_SOLVER_STACK_FRAMES {
            // Curtailment: the caller bails with a relation-preserving default
            // and a nested-eval bail this way is invisible to an outer
            // instantiator's own limit flags. Bump the monotonic counter so the
            // instantiation cache can detect a curtailed walk (see
            // `solver_frame_bail_count`).
            b.solver_frame_bail_count
                .set(b.solver_frame_bail_count.get().saturating_add(1));
            false
        } else {
            b.solver_stack_frames.set(depth + 1);
            true
        }
    })
}

/// Monotonic count of solver-frame-budget curtailments on this thread (see
/// [`LimitBudgets::solver_frame_bail_count`]). The project-wide instantiation
/// cache snapshots this before/after a walk and refuses to store a result whose
/// computation saw a curtailment.
#[inline]
pub(crate) fn solver_frame_bail_count() -> u32 {
    LIMIT_BUDGETS.with(|b| b.solver_frame_bail_count.get())
}

/// Release one cross-operation solver recursion frame.
#[inline]
pub(crate) fn solver_stack_frame_leave() {
    LIMIT_BUDGETS.with(|b| {
        b.solver_stack_frames
            .set(b.solver_stack_frames.get().saturating_sub(1));
    });
}

/// Current number of active solver recursion frames on this thread.
#[inline]
pub(crate) fn solver_stack_frame_depth() -> u32 {
    LIMIT_BUDGETS.with(|b| b.solver_stack_frames.get())
}

/// Reset the thread-local solver frame counter to zero (defensive backstop
/// against drift from a swallowed panic; see `recursion.rs`).
#[inline]
pub(crate) fn reset_solver_stack_frames() {
    LIMIT_BUDGETS.with(|b| b.solver_stack_frames.set(0));
}

// =============================================================================
// Limit-hit result-cache policy (issue #13241)
// =============================================================================

/// Whether limit-hit relation/eval outcomes may be recorded and reused
/// (kill switch for the issue #13241 policy).
///
/// When a relation or evaluation chain hits a recursion/depth/fuel limit,
/// `tsc` records `Ternary.Maybe` outcomes (its `maybeKeys` stack) and
/// promotes them to cached successes once the outermost relation completes
/// successfully. `tsz` mirrors that policy with:
///
/// - the maybe-stack promotion in `relations::subtype::cache` (cycle-derived
///   `Maybe` keys promoted to definitive `true`, fuel-derived `Maybe` keys
///   promoted to band-conditional `RelationCacheValue::LimitTrue` entries,
///   the band measured against [`remaining_global_subtype_fuel`]), and
/// - the per-intermediate taint discrimination in the evaluator
///   (`TypeEvaluator` tainted set), which lets clean intermediate evaluation
///   results persist even when an unrelated subtree hit a limit.
///
/// Any future promotion of additional guard classes (per-class `Maybe`
/// caching for evaluation or instantiation limits) must route through this
/// switch and this module's budget accessors so the policy stays in one
/// place.
///
/// Enabled by default; set `TSZ_DISABLE_LIMIT_RESULT_CACHE=1` to restore the
/// previous drop-everything-on-limit-hit behavior for cache-on/off A/B
/// verification.
pub(crate) fn limit_result_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| !std::env::var("TSZ_DISABLE_LIMIT_RESULT_CACHE").is_ok_and(|v| v == "1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #13368: batch/row reuse runs many compilations on
    /// one worker thread and relies on [`reset_subtype_thread_local_state`] to
    /// isolate them. A compilation that bailed via the depth/stack breaker or a
    /// caught-and-swallowed panic can leave any [`LimitBudgets`] counter dirty.
    /// Before the fix the reset cleared only three of the eight fields, so the
    /// per-query op budget, cross-evaluator eval depth, per-file evaluation
    /// fuel, and solver recursion-frame count leaked into the next compilation
    /// and made its depth/fuel verdicts schedule-dependent. Dirty every field
    /// the way an aborted mid-evaluation row would and assert the reset zeroes
    /// all of them.
    #[test]
    fn reset_clears_every_limit_budget_field() {
        LIMIT_BUDGETS.with(|b| {
            b.subtype_state.set(pack_depth_fuel(7, 9));
            b.lazy_resolve_failures.set(3);
            b.weak_type_sensitivity.set(5);
            b.eval_query_active.set(11);
            b.eval_query_ops.set(13);
            b.global_eval_depth.set(17);
            b.evaluation_fuel.set(19);
            b.solver_stack_frames.set(23);
        });

        reset_subtype_thread_local_state();

        LIMIT_BUDGETS.with(|b| {
            assert_eq!(b.subtype_state.get(), 0, "subtype chain state");
            assert_eq!(b.lazy_resolve_failures.get(), 0, "lazy-resolve sentinel");
            assert_eq!(b.weak_type_sensitivity.get(), 0, "weak-type sentinel");
            assert_eq!(b.eval_query_active.get(), 0, "per-query active frames");
            assert_eq!(b.eval_query_ops.get(), 0, "per-query op count");
            assert_eq!(b.global_eval_depth.get(), 0, "cross-evaluator eval depth");
            assert_eq!(b.evaluation_fuel.get(), 0, "per-file evaluation fuel");
            assert_eq!(b.solver_stack_frames.get(), 0, "solver recursion frames");
        });
    }

    /// The packed subtype chain state round-trips depth and fuel through the
    /// consolidated accessors with the exact pre/post semantics the relation
    /// cache relies on.
    #[test]
    fn subtype_frame_enter_leave_round_trip() {
        // Ensure a clean slate even if another test on this thread leaked.
        reset_subtype_thread_local_state();

        let outer = enter_subtype_frame();
        assert_eq!(outer.global_depth, 0, "first frame is outermost");
        assert_eq!(outer.fuel, 0, "no fuel consumed before the first frame");

        let inner = enter_subtype_frame();
        assert_eq!(inner.global_depth, 1);
        assert_eq!(inner.fuel, 1);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL - 2,
            "two frames consumed two fuel units"
        );

        leave_subtype_frame(false);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL - 2,
            "fuel is monotonic until the outermost frame exits"
        );

        leave_subtype_frame(true);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL,
            "outermost exit resets the chain fuel"
        );
    }

    /// Sentinel counters advance independently and are visible through both
    /// the single-counter and the combined snapshot accessors.
    #[test]
    fn poison_sentinels_advance_and_combine() {
        let (lazy0, weak0) = poison_sentinel_counts();
        note_lazy_resolve_failure();
        note_weak_type_sensitivity();
        note_weak_type_sensitivity();
        let (lazy1, weak1) = poison_sentinel_counts();
        assert_eq!(lazy1, lazy0 + 1);
        assert_eq!(weak1, weak0 + 2);
        assert_eq!(lazy_resolve_failure_count(), lazy1);
    }
}
