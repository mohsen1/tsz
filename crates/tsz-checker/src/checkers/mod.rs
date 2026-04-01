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
/// indices (NodeIndex) as keys, and these indices get reused across
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
