//! Amortized stack-overflow guard for the binder's recursive AST walk.
//!
//! The binder (`bind_node`) calls itself recursively once per AST node. On
//! deeply nested inputs (e.g. hundreds of chained arrow functions) the call
//! stack can overflow. `stacker::maybe_grow` fixes the immediate crash by
//! allocating a fresh stack segment on demand, but calling it unconditionally
//! on every node is wasteful - the probe itself has non-trivial overhead.
//!
//! This module provides a thread-local state word that amortizes the probe
//! cost: `maybe_grow` is only invoked when `should_probe_stack()` returns
//! `true` (every 64th call), and a one-way "tripped" breaker prevents
//! `maybe_grow` from ever being called again on a thread that has already
//! detected critically-low stack.
//!
//! Packed thread-local: bit 15 = tripped flag, bits 0..7 = probe counter.
//! This costs one TLV access per call instead of two separate `thread_local!`
//! reads, matching the pattern used by the checker (`checkers/mod.rs`).
//!
//! Call `reset_stack_overflow_flag()` between independent file binds (e.g.
//! from `BinderState::bind_source_file`) so a pathological file does not
//! permanently disable stack safety for subsequent files on the same thread.

thread_local! {
    static BINDER_STACK_STATE: std::cell::Cell<u16> = const { std::cell::Cell::new(0) };
}

const BINDER_STACK_TRIPPED_BIT: u16 = 0x8000;
const BINDER_STACK_COUNTER_MASK: u16 = 0x3F; // probe every 64th call

/// Returns `true` if the stack overflow breaker has been tripped on this thread.
///
/// When tripped, the binder should return immediately without processing
/// further nodes to prevent a stack overflow.
#[inline]
pub(crate) fn stack_overflow_tripped() -> bool {
    BINDER_STACK_STATE.get() & BINDER_STACK_TRIPPED_BIT != 0
}

/// Returns `true` if the stack should be probed on this call.
///
/// Returns `true` every 64th invocation, amortizing the cost of
/// `stacker::remaining_stack()` across ordinary non-recursive call sites.
#[inline]
pub(crate) fn should_probe_stack() -> bool {
    let s = BINDER_STACK_STATE.get();
    let c = (s & 0xFF).wrapping_add(1) & 0xFF;
    BINDER_STACK_STATE.set((s & BINDER_STACK_TRIPPED_BIT) | c);
    c & BINDER_STACK_COUNTER_MASK == 0
}

/// Trip the stack overflow breaker for this thread.
///
/// Called when `stacker::remaining_stack()` reports critically low stack
/// headroom. Subsequent calls to `stack_overflow_tripped()` will return
/// `true` until the flag is explicitly reset.
#[inline]
pub(crate) fn trip_stack_overflow() {
    BINDER_STACK_STATE.set(BINDER_STACK_STATE.get() | BINDER_STACK_TRIPPED_BIT);
}

/// Reset the stack overflow breaker for this thread.
///
/// Must be called at the start of each file's `bind_source_file` so that a
/// pathological file does not permanently disable stack safety for
/// subsequent files processed on the same thread.
pub(crate) fn reset_stack_overflow_flag() {
    BINDER_STACK_STATE.set(BINDER_STACK_STATE.get() & !BINDER_STACK_TRIPPED_BIT);
}
