//! Cross-arena interface-delegation depth guard.
//!
//! Extracted from `cross_file.rs` to keep that module under the 2000-line
//! maintainability limit. Tracks the recursion depth of cross-arena interface
//! delegation so deeply nested child-checker creation cannot overflow the
//! stack, and exposes a reset hook for independent-compilation boundaries.

use crate::state::CheckerState;

thread_local! {
    static CROSS_ARENA_INTERFACE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Reset the cross-arena interface-delegation depth counter to zero.
///
/// `enter_cross_arena_interface_delegation` / `leave_...` form a manual
/// (non-RAII) pair, so an early bail-out between them (stack-overflow breaker,
/// resolution fuel exhaustion) can leave the counter non-zero and suppress
/// interface delegation in a later, unrelated compilation. Reset between
/// independent compilations in batch mode.
pub(crate) fn reset_cross_arena_interface_depth() {
    CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(0));
}

#[cfg(test)]
pub(crate) fn set_cross_arena_interface_depth_for_test(value: u32) {
    CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(value));
}

#[cfg(test)]
pub(crate) fn cross_arena_interface_depth_for_test() -> u32 {
    CROSS_ARENA_INTERFACE_DEPTH.with(std::cell::Cell::get)
}

impl<'a> CheckerState<'a> {
    pub(crate) fn enter_cross_arena_interface_delegation() {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(c.get() + 1));
    }

    pub(crate) fn leave_cross_arena_interface_delegation() {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
    }

    pub(crate) fn in_cross_arena_interface_delegation() -> bool {
        CROSS_ARENA_INTERFACE_DEPTH.with(|c| c.get() > 0)
    }
}
