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
pub fn clear_all_thread_local_state() {
    // Reset stack overflow breaker
    STACK_STATE.set(0);

    // Clear enum evaluation memos (use NodeIndex keys that are arena-local)
    crate::types_domain::utilities::enum_utils::clear_enum_eval_memo();
    crate::types_domain::utilities::const_enum_eval::clear_const_eval_memo();

    // Clear cycle guard visited sets
    crate::types_domain::utilities::cycle_guard::clear_visited_sets();

    // Reset resolution fuel and depth counters
    crate::state_domain::type_environment::lazy::reset_all_thread_local_state();
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
}
