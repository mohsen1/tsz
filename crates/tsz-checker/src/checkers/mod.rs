//! Domain-specific checker modules.
//!
//! Each module implements type-checking logic for a particular language feature,
//! delegating type-semantic queries to the solver via `query_boundaries`.

pub mod accessor_checker;
pub mod call_checker;
pub mod call_context;
pub mod enum_checker;
pub mod generic_checker;
pub mod iterable_checker;
pub mod jsx;
pub mod parameter_checker;
pub mod promise_checker;
mod promise_checker_generator;
mod promise_checker_object_normalization;
pub mod property_checker;
pub mod signature_builder;

use tsz_parser::parser::base::NodeIndex;
use tsz_solver::TypeId;

// ── Stack-overflow breaker ──────────────────────────────────────────────
// Shared thread-local flag set when stacker::remaining_stack() detects
// critically low stack.  Once tripped, all guarded recursive entry points
// bail with TypeId::ERROR for the remainder of this thread's lifetime.
// This prevents both the initial crash AND the hang that would otherwise
// result when the cycle re-enters at shallow depth.
//
// Reset between files in batch mode via `reset_stack_overflow_flag()`.
// Packed thread-local: bit 15 = tripped flag, bits 0..7 = probe counter.
// Single TLV access instead of two separate thread_locals.
thread_local! {
    static STACK_STATE: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
}

const STACK_TRIPPED_BIT: u16 = 0x8000;
const STACK_COUNTER_MASK: u16 = 0x0F; // 16-element cycle (probe every 16th call)

/// Returns `true` if the stack overflow breaker has been tripped.
#[inline]
pub fn stack_overflow_tripped() -> bool {
    STACK_STATE.get() & STACK_TRIPPED_BIT != 0
}

/// Returns `true` if the stack should be probed on this call.
/// Amortizes the `stacker::remaining_stack()` cost by only returning
/// `true` every 64th invocation.
#[inline]
pub fn should_probe_stack() -> bool {
    let s = STACK_STATE.get();
    let c = (s & 0xFF).wrapping_add(1) & 0xFF;
    STACK_STATE.set((s & STACK_TRIPPED_BIT) | c);
    c & STACK_COUNTER_MASK == 0
}

/// Decide whether measured stack headroom is below `min_bytes`.
///
/// Pure function of the measurement so the `None` case is unit-testable without
/// controlling the real stack. `stacker::remaining_stack()` returns `None` on
/// targets whose stack bounds cannot be determined (notably `wasm32`). Treating
/// that unknown as "critically low" (the old `unwrap_or(0)` form) trips the
/// breaker on the very first probe, aborting the recursive walk mid-input and
/// silently dropping every node after the trip point. With no measurement there
/// is no evidence of exhaustion, so we report "not low" and let the walk
/// proceed; `stacker::maybe_grow` still handles genuine growth.
#[inline]
#[must_use]
pub const fn measured_headroom_below(remaining: Option<usize>, min_bytes: usize) -> bool {
    matches!(remaining, Some(r) if r < min_bytes)
}

/// `true` only when the remaining stack can be measured AND is below `min_bytes`.
#[inline]
#[must_use]
pub fn headroom_below(min_bytes: usize) -> bool {
    measured_headroom_below(stacker::remaining_stack(), min_bytes)
}

/// Remaining-stack headroom below which a guarded entry point trips the breaker
/// and bails. Sits well above the `stacker::maybe_grow` red zone so the breaker
/// engages before a fresh segment would be needed for the next deep frame.
const STACK_BAILOUT_HEADROOM_BYTES: usize = 1024 * 1024;
/// `stacker::maybe_grow` red-zone: grow the stack once free headroom drops below
/// this many bytes. Shared by every guarded checker recursion site.
const STACK_RED_ZONE_BYTES: usize = 256 * 1024;
/// `stacker::maybe_grow` new-segment size for guarded checker recursion sites.
const STACK_GROW_BYTES: usize = 2 * 1024 * 1024;

/// Run `body` under the shared checker stack-overflow breaker.
///
/// This is the single owner of the probe → trip → grow sequence that guards
/// every deep, *cross-context* recursive checker entry point (expression
/// dispatch, interface heritage merging, …). The thread-local breaker is the
/// only stack-safety mechanism that survives the fresh / cross-arena child
/// `CheckerContext`s spun up while resolving a base symbol — the per-context
/// logical depth `Cell`s (`heritage_merge_depth`, `enter_recursion`) reset to
/// zero across those boundaries, so a recursion that hops contexts accumulates
/// real OS-stack frames that no per-context counter ever bounds (#14111).
///
/// Behaviour, mirroring the original inline `dispatch_type_computation` guard:
/// 1. If the breaker has already tripped on this thread, return `on_exhausted`
///    immediately (no further recursion for the rest of the file).
/// 2. On a periodic [`should_probe_stack`] tick, measure remaining stack; if it
///    is below [`STACK_BAILOUT_HEADROOM_BYTES`], trip the shared breaker and
///    return `on_exhausted`.
/// 3. Otherwise run `body` on a (possibly freshly grown) stack via
///    `stacker::maybe_grow`.
///
/// `on_exhausted` is the safe, relation-preserving value the caller would have
/// returned anyway when its own logical depth guard fires (e.g. the partially
/// merged `derived_type`, or `TypeId::ERROR` for expression dispatch).
#[inline]
pub fn with_stack_guard<T>(on_exhausted: T, body: impl FnOnce() -> T) -> T {
    if stack_overflow_tripped() {
        return on_exhausted;
    }
    if should_probe_stack() && headroom_below(STACK_BAILOUT_HEADROOM_BYTES) {
        trip_stack_overflow();
        return on_exhausted;
    }
    stacker::maybe_grow(STACK_RED_ZONE_BYTES, STACK_GROW_BYTES, body)
}

/// Trip the stack overflow breaker.  Called from guards in `dispatch.rs` and
/// `state/type_analysis/core.rs` when `stacker::remaining_stack()` reports
/// < 256 KB remaining.
pub fn trip_stack_overflow() {
    STACK_STATE.set(STACK_STATE.get() | STACK_TRIPPED_BIT);
}

/// Reset the breaker.  Called between files in batch mode so that one
/// pathological file doesn't poison all subsequent files.
pub fn reset_stack_overflow_flag() {
    STACK_STATE.set(STACK_STATE.get() & !STACK_TRIPPED_BIT);
}

/// Clear all thread-local state in the checker.
///
/// MUST be called between independent compilation sessions (e.g., in batch
/// mode) to prevent stale cached entries from a previous compilation from
/// affecting subsequent compilations. Thread-local caches use arena-local
/// indices (`NodeIndex`) as keys, and these indices get reused across
/// compilations, causing cross-compilation contamination.
///
/// This is a hand-maintained reset list: every per-compilation thread-local in
/// the checker (memo, scratch pool, or recursion-guard depth/stack) must be
/// reset here. Rust offers no way to enumerate `thread_local!`s, so each owning
/// module exposes a `reset_*` function and a parent `mod.rs` aggregates them;
/// this function calls the aggregators. When you add a new per-compilation
/// thread-local, add its reset here too — the regression test
/// `clear_all_thread_local_state_resets_cross_arena_and_alias_guards` guards the
/// recursion-guard subset against drift.
///
/// Most of the reset surface — every recursion/cycle guard and depth counter —
/// is delegated to [`reset_per_file_resolution_guards`], which the fresh-checker
/// path also runs at every *file* boundary so a mid-walk bail cannot leak dirty
/// guard state onto a shared worker thread (see that function's docs). This
/// function additionally drops the warm cross-file memos that are only stale at
/// a *compilation* boundary.
pub fn clear_all_thread_local_state() {
    // Recursion/cycle guards, depth counters, and arena-local scratch memos.
    reset_per_file_resolution_guards();

    // Drop the module-specifier candidate memo. It is a pure function of the
    // specifier text (no correctness dependence on row state), but clearing at
    // row boundaries keeps it from accumulating every project's specifiers on a
    // reused worker thread while preserving within-compilation cross-file reuse.
    // Unlike the guards above this is a *warm* cross-file memo, so it is dropped
    // only at the compilation boundary, never per file.
    crate::module_resolution::reset_module_specifier_candidates_memo();
}

/// Reset the checker's transient per-file resolution guards, depth counters, and
/// arena-local visited-sets/memos on the current thread.
///
/// Every entry here is balanced (RAII or manual enter/leave / push/pop) in the
/// normal path, so it is empty or zero at a clean file boundary. A mid-walk bail
/// — the stack-overflow breaker, fuel/recursion-limit exhaustion, or a panic
/// caught by the batch driver — can instead leave a depth counter non-zero or a
/// visited-set non-empty.
///
/// The fresh-checker path checks files on shared rayon pool worker threads
/// (`check_file_for_parallel`, used by both the sequential and parallel fresh
/// arms), and reuses those threads across files within a compilation and across
/// compilations in a batch worker. A dirty guard left by one file would suppress
/// resolution in the *next* file scheduled onto the same worker thread —
/// nondeterministically, because file→worker assignment depends on the parallel
/// schedule. Running this reset at every file boundary makes a bail in one file
/// unable to leak into another on any thread, closing a documented source of the
/// schedule-sensitive conformance flakes (#13255 / #13368 family).
///
/// This deliberately does **not** drop the warm cross-file memos (e.g. the
/// module-specifier candidate memo): those are pure functions of their inputs
/// and are safe — and beneficial — to keep warm across files within a
/// compilation. Only [`clear_all_thread_local_state`] (the per-compilation
/// reset) drops those.
pub fn reset_per_file_resolution_guards() {
    // Reset stack overflow breaker
    STACK_STATE.set(0);

    // Clear enum evaluation memos (use NodeIndex keys that are arena-local)
    crate::types_domain::utilities::enum_utils::clear_enum_eval_memo();
    crate::types_domain::utilities::const_enum_eval::clear_const_eval_memo();

    // Clear cycle guard visited sets
    crate::types_domain::utilities::cycle_guard::clear_visited_sets();

    // Reset resolution fuel and depth counters
    crate::state_domain::type_environment::lazy::reset_all_thread_local_state();

    // Reset cross-arena/cross-file recursion-guard depth counters and stacks.
    // These use manual (non-RAII) enter/leave or push/pop, so a project that
    // bails out mid-delegation (stack-overflow breaker, fuel exhaustion, or a
    // panic caught by the batch driver) can leave them dirty and suppress
    // resolution in the next project on this worker thread.
    crate::state_domain::state::reset_cross_arena_depth();
    crate::state_domain::type_analysis::reset_cross_file_recursion_guards();

    // Reset the contextual-retry cache-invalidation recursion-depth counter.
    // The RAII guard self-cleans on scope exit and panic unwind, so this is
    // normally a no-op, but resetting at row boundaries makes isolation total
    // against any future non-unwinding bail-out from inside the walker.
    crate::state_domain::cache_invalidation::reset_contextual_retry_path();

    // Reset type-alias resolution recursion guards and scratch pools.
    crate::types_domain::reset_type_resolution_guards();

    // Reset lib-resolution marks plus the heritage-cycle drain depth/draining
    // counters. The depth counter is balanced in the normal path, but a
    // mid-resolution bail-out could leave it non-zero and suppress the cycle
    // drain for every later row on this worker thread (#12299).
    crate::types_domain::queries::lib_resolution::reset_lib_resolution_state();

    // Reset the Awaited<…> assignability-normalization cycle guard and clamp
    // epoch. The visiting set keys on arena-local `TypeId`s reused across
    // compilations; a leaked entry from a mid-walk bail would suppress
    // normalization for a colliding fresh `TypeId` on this worker thread.
    self::promise_checker_object_normalization::reset_awaited_eval_thread_local_state();
}

/// Explicit context for synthesized JSX children, threaded from dispatch
/// into the JSX checking path instead of stored as ambient mutable state
/// on `CheckerContext`.
#[derive(Clone)]
pub struct JsxChildrenContext {
    /// Number of children in the JSX body.
    pub child_count: usize,
    /// Whether any `JsxText` children exist.
    pub has_text_child: bool,
    /// The contextual `children` type computed before body children are evaluated.
    pub contextual_type: Option<TypeId>,
    /// The type to use as the `children` prop value.
    pub synthesized_type: TypeId,
    /// Node indices of `JsxText` children (for TS2747 location reporting).
    pub text_child_indices: Vec<NodeIndex>,
}

#[cfg(test)]
mod tests {
    //! Stack-overflow breaker thread-local state tests.
    //!
    //! Each `#[test]` runs on its own thread under nextest, so the
    //! `STACK_STATE` thread-local starts at 0 for every test. Tests that
    //! mutate global thread-locals must still reset state at the end so
    //! repeated invocations under `cargo test` (single-threaded harness)
    //! don't pollute each other.
    use super::*;

    fn reset() {
        STACK_STATE.set(0);
    }

    #[test]
    fn stack_overflow_tripped_starts_false() {
        reset();
        assert!(!stack_overflow_tripped());
    }

    #[test]
    fn unmeasurable_headroom_never_trips() {
        // `wasm32` reports `None`; that must not count as critically low, or the
        // checker bails to `TypeId::ERROR` mid-recursion on every guarded entry
        // point (issue #13815 family).
        assert!(!measured_headroom_below(None, 1024 * 1024));
        assert!(!measured_headroom_below(None, usize::MAX));
    }

    #[test]
    fn measured_headroom_compares_against_threshold() {
        assert!(measured_headroom_below(Some(256 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(4 * 1024 * 1024), 1024 * 1024));
        assert!(!measured_headroom_below(Some(1024 * 1024), 1024 * 1024));
    }

    #[test]
    fn trip_stack_overflow_flips_tripped_flag() {
        reset();
        assert!(!stack_overflow_tripped());
        trip_stack_overflow();
        assert!(stack_overflow_tripped());
        reset();
    }

    #[test]
    fn reset_stack_overflow_flag_clears_tripped_bit_only() {
        reset();
        // Increment the probe counter a few times.
        for _ in 0..10 {
            should_probe_stack();
        }
        let counter_before_reset = STACK_STATE.get() & 0xFF;
        assert_ne!(counter_before_reset, 0, "counter should have advanced");

        trip_stack_overflow();
        assert!(stack_overflow_tripped());

        reset_stack_overflow_flag();
        // Tripped bit cleared but counter preserved (bit 15 != counter).
        assert!(!stack_overflow_tripped());
        assert_eq!(
            STACK_STATE.get() & 0xFF,
            counter_before_reset,
            "reset must not clear the probe counter"
        );
        reset();
    }

    #[test]
    fn should_probe_stack_returns_true_every_16th_call() {
        reset();
        // The counter increments on every call; the helper returns true
        // when `counter & 0x0F == 0`. Starting from 0, the FIRST call
        // increments to 1, returns false. The 16th call increments the
        // counter to 16, which `& 0x0F == 0`, returns true.
        let mut hits = 0usize;
        for _ in 0..32 {
            if should_probe_stack() {
                hits += 1;
            }
        }
        // Out of 32 increments (1..=32), exactly 2 of those values
        // (16 and 32) have `(counter & 0x0F) == 0`.
        assert_eq!(
            hits, 2,
            "should_probe_stack should return true exactly 2 times in 32 calls"
        );
        reset();
    }

    #[test]
    fn should_probe_stack_first_call_is_false() {
        reset();
        // First call: counter goes 0 → 1. `1 & 0x0F == 1`, so returns false.
        assert!(!should_probe_stack());
        reset();
    }

    #[test]
    fn should_probe_stack_preserves_tripped_bit() {
        reset();
        trip_stack_overflow();
        assert!(stack_overflow_tripped());
        // Run probe-stack many times — the tripped bit must survive.
        for _ in 0..20 {
            should_probe_stack();
        }
        assert!(
            stack_overflow_tripped(),
            "tripped bit must be preserved across should_probe_stack calls"
        );
        reset();
    }

    #[test]
    fn counter_wraps_at_byte_boundary() {
        reset();
        // The counter masks with 0xFF, so it wraps after 256 calls back to
        // 0 (whose `& 0x0F == 0` → returns true on call 256).
        for _ in 0..255 {
            should_probe_stack();
        }
        // After 255 calls, counter == 255. Call 256: 255 + 1 = 256, masked
        // to 0. `0 & 0x0F == 0` → true.
        assert!(should_probe_stack());
        reset();
    }

    #[test]
    fn with_stack_guard_runs_body_when_healthy() {
        reset();
        let mut ran = false;
        let out = with_stack_guard(-1, || {
            ran = true;
            42
        });
        assert!(ran, "body must run when the breaker is healthy");
        assert_eq!(out, 42, "with_stack_guard must return the body result");
        reset();
    }

    #[test]
    fn with_stack_guard_bails_without_running_body_when_tripped() {
        reset();
        trip_stack_overflow();
        let mut ran = false;
        let out = with_stack_guard(-1, || {
            ran = true;
            42
        });
        assert!(
            !ran,
            "body must NOT run once the shared breaker has tripped — this is what \
             unwedges the cross-context heritage-merge recursion (#14111)"
        );
        assert_eq!(
            out, -1,
            "with_stack_guard must return on_exhausted when tripped"
        );
        reset();
    }

    #[test]
    fn with_stack_guard_does_not_trip_on_probe_ticks_when_headroom_is_ample() {
        reset();
        // Drive enough calls to cross several probe ticks. A real test thread has
        // ample headroom, so none of the probes should trip the breaker.
        for _ in 0..64 {
            let out = with_stack_guard(0u32, || 7u32);
            assert_eq!(out, 7, "body must keep running while headroom is ample");
        }
        assert!(
            !stack_overflow_tripped(),
            "ample-headroom probes must not trip the breaker"
        );
        reset();
    }

    #[test]
    fn clear_all_thread_local_state_zeros_stack_state() {
        // Trip the breaker and advance the counter, then clear.
        trip_stack_overflow();
        for _ in 0..5 {
            should_probe_stack();
        }
        assert!(stack_overflow_tripped());
        clear_all_thread_local_state();
        assert!(
            !stack_overflow_tripped(),
            "clear_all_thread_local_state must clear the tripped bit"
        );
        assert_eq!(
            STACK_STATE.get(),
            0,
            "clear_all_thread_local_state must zero the entire STACK_STATE"
        );
    }

    /// Regression test for issue #10880: batch mode reuses one worker process
    /// across project rows and relies on `clear_all_thread_local_state` to
    /// isolate them. Several cross-arena / type-alias recursion guards use
    /// manual (non-RAII) enter/leave or push/pop, so a row that bails out
    /// mid-delegation can leave them dirty. Before the fix these guards were
    /// not part of the reset, leaking state into the next row. This asserts
    /// every such guard is zero/empty after the canonical reset.
    #[test]
    fn clear_all_thread_local_state_resets_cross_arena_and_alias_guards() {
        use crate::{state_domain, types_domain};

        // Dirty every guard the way an aborted mid-delegation row would.
        state_domain::state::set_cross_arena_depth_for_test(3);
        state_domain::state::set_cross_arena_bailout_epoch_for_test(7);
        state_domain::type_analysis::dirty_cross_file_recursion_guards_for_test();
        types_domain::dirty_type_resolution_guards_for_test();
        super::promise_checker_object_normalization::dirty_awaited_eval_thread_local_state_for_test(
        );

        assert_ne!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "precondition: cross-arena depth dirtied"
        );
        assert!(
            !state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "precondition: cross-file recursion guards dirtied"
        );
        assert!(
            !types_domain::type_resolution_guards_clear_for_test(),
            "precondition: type-resolution guards dirtied"
        );
        assert!(
            !super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "precondition: awaited-eval thread-locals dirtied"
        );

        clear_all_thread_local_state();

        assert_eq!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "clear_all_thread_local_state must reset the cross-arena delegation depth"
        );
        assert_eq!(
            state_domain::state::cross_arena_bailout_epoch_for_test(),
            0,
            "clear_all_thread_local_state must reset the cross-arena bailout epoch"
        );
        assert!(
            state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "clear_all_thread_local_state must reset cross-file interface depth and alias stack"
        );
        assert!(
            types_domain::type_resolution_guards_clear_for_test(),
            "clear_all_thread_local_state must reset alias-resolution depth, stack, and scratch pool"
        );
        assert!(
            super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "clear_all_thread_local_state must reset the awaited-eval cycle guard and clamp epoch"
        );
    }

    /// The fresh-checker path runs [`reset_per_file_resolution_guards`] at every
    /// *file* boundary (in `check_file_for_parallel` / `reset_for_next_file`) so
    /// a file that bails mid-walk cannot leak a dirty recursion/cycle guard into
    /// the next file scheduled onto the same shared worker thread (#13255 /
    /// #13368 schedule-sensitivity). This asserts the per-file reset clears the
    /// same cross-arena / alias / awaited guards the compilation-boundary reset
    /// does — they must not survive a file boundary on any thread.
    #[test]
    fn reset_per_file_resolution_guards_clears_cross_arena_and_alias_guards() {
        use crate::{state_domain, types_domain};

        // Dirty every guard the way an aborted mid-walk file would.
        state_domain::state::set_cross_arena_depth_for_test(3);
        state_domain::state::set_cross_arena_bailout_epoch_for_test(7);
        state_domain::type_analysis::dirty_cross_file_recursion_guards_for_test();
        types_domain::dirty_type_resolution_guards_for_test();
        super::promise_checker_object_normalization::dirty_awaited_eval_thread_local_state_for_test(
        );

        reset_per_file_resolution_guards();

        assert_eq!(
            state_domain::state::cross_arena_depth_for_test(),
            0,
            "per-file reset must clear the cross-arena delegation depth"
        );
        assert_eq!(
            state_domain::state::cross_arena_bailout_epoch_for_test(),
            0,
            "per-file reset must clear the cross-arena bailout epoch"
        );
        assert!(
            state_domain::type_analysis::cross_file_recursion_guards_clear_for_test(),
            "per-file reset must clear cross-file interface depth and alias stack"
        );
        assert!(
            types_domain::type_resolution_guards_clear_for_test(),
            "per-file reset must clear alias-resolution depth, stack, and scratch pool"
        );
        assert!(
            super::promise_checker_object_normalization::awaited_eval_thread_local_state_clear_for_test(),
            "per-file reset must clear the awaited-eval cycle guard and clamp epoch"
        );
    }

    /// `enter_cross_arena_delegation` must record a bailout (advance the
    /// bailout epoch) when it refuses delegation at the depth cap, and must
    /// NOT advance it on an allowed delegation. The epoch is what
    /// `delegate_cross_arena_symbol_resolution` / `get_type_of_symbol` compare
    /// before/after a resolution to decide whether a provisional sentinel was
    /// minted under the cap and must be kept out of the persistent caches
    /// (#13846). Name-agnostic: exercises the guard primitive directly.
    #[test]
    fn cross_arena_depth_cap_records_bailout_epoch() {
        use crate::CheckerState;
        use crate::state_domain::state;

        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);

        // Below the cap: delegation is allowed and does not record a bailout.
        let guard = CheckerState::<'_>::enter_cross_arena_delegation()
            .expect("delegation below the depth cap must be allowed");
        drop(guard);
        assert_eq!(
            state::cross_arena_bailout_epoch_for_test(),
            0,
            "an allowed delegation must not record a bailout"
        );

        // At the cap: delegation is refused and records a bailout so the
        // enclosing resolution refuses to persist its incomplete result.
        state::set_cross_arena_depth_for_test(5);
        let before = state::cross_arena_bailout_epoch_for_test();
        assert!(
            CheckerState::<'_>::enter_cross_arena_delegation().is_none(),
            "delegation at the depth cap must be refused"
        );
        assert_ne!(
            state::cross_arena_bailout_epoch_for_test(),
            before,
            "a depth-cap bailout must advance the bailout epoch"
        );

        // Leave the thread-locals clean for sibling tests on this worker.
        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);
    }

    /// The cross-arena depth guard is RAII-owned: early returns and unwinds drop
    /// the guard instead of relying on every caller to remember a matching
    /// manual leave.
    #[test]
    fn cross_arena_delegation_guard_restores_depth_on_drop() {
        use crate::CheckerState;
        use crate::state_domain::state;

        state::set_cross_arena_depth_for_test(0);
        state::set_cross_arena_bailout_epoch_for_test(0);

        {
            let _outer = CheckerState::<'_>::enter_cross_arena_delegation()
                .expect("outer delegation should enter");
            assert_eq!(state::cross_arena_depth_for_test(), 1);
            {
                let _inner = CheckerState::<'_>::enter_cross_arena_delegation()
                    .expect("nested delegation should enter");
                assert_eq!(state::cross_arena_depth_for_test(), 2);
            }
            assert_eq!(state::cross_arena_depth_for_test(), 1);
        }

        assert_eq!(
            state::cross_arena_depth_for_test(),
            0,
            "dropping the guards must restore the shared depth"
        );
        assert_eq!(
            state::cross_arena_bailout_epoch_for_test(),
            0,
            "successful guard scopes must not record a bailout"
        );
    }
}
