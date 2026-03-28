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
thread_local! {
    static STACK_OVERFLOW_TRIPPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Counter for amortizing the `stacker::remaining_stack()` syscall.
    /// We only probe the real stack depth every Nth call.
    static STACK_CHECK_COUNTER: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Returns `true` if the stack overflow breaker has been tripped.
#[inline]
pub fn stack_overflow_tripped() -> bool {
    STACK_OVERFLOW_TRIPPED.get()
}

/// Returns `true` if the stack should be probed on this call.
/// Amortizes the `stacker::remaining_stack()` cost by only returning
/// `true` every 64th invocation.
#[inline]
pub fn should_probe_stack() -> bool {
    let c = STACK_CHECK_COUNTER.get().wrapping_add(1);
    STACK_CHECK_COUNTER.set(c);
    c & 63 == 0
}

/// Trip the stack overflow breaker.  Called from guards in `dispatch.rs` and
/// `state/type_analysis/core.rs` when `stacker::remaining_stack()` reports
/// < 256 KB remaining.
pub fn trip_stack_overflow() {
    STACK_OVERFLOW_TRIPPED.set(true);
}

/// Reset the breaker.  Called between files in batch mode so that one
/// pathological file doesn't poison all subsequent files.
pub fn reset_stack_overflow_flag() {
    STACK_OVERFLOW_TRIPPED.set(false);
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
    /// The type to use as the `children` prop value.
    pub synthesized_type: TypeId,
    /// Node indices of `JsxText` children (for TS2747 location reporting).
    pub text_child_indices: Vec<NodeIndex>,
}
