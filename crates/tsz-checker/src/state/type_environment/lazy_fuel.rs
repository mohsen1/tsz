//! Global fuel bookkeeping for lazy type resolution.

thread_local! {
    // Global accumulating fuel counter that does NOT reset between top-level
    // ensure_relation_input_ready calls. Prevents OOM when many top-level calls
    // each reset per-call fuel but together create unbounded type data
    // (e.g., DOM types + module augmentation in reactTransitiveImportHasValidDeclaration).
    static GLOBAL_RESOLUTION_FUEL: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

// Maximum global resolution fuel across all top-level calls per thread.
// This must be high enough to process large files with many expressions
// (e.g., unionSubtypeReductionErrors.ts has 6000+ lines requiring ~15K+
// resolution ops). DOM-heavy React code with module augmentations can
// explode to hundreds of thousands; this limit prevents that while
// allowing legitimate large files.
const MAX_GLOBAL_RESOLUTION_FUEL: u32 = 50_000;

/// Check if global resolution fuel is exhausted.
pub(crate) fn global_resolution_fuel_exhausted() -> bool {
    GLOBAL_RESOLUTION_FUEL.get() >= MAX_GLOBAL_RESOLUTION_FUEL
}

/// Increment the global resolution fuel counter.
pub(crate) fn increment_global_resolution_fuel() {
    GLOBAL_RESOLUTION_FUEL.set(GLOBAL_RESOLUTION_FUEL.get() + 1);
}

/// Reset global resolution fuel (call at the start of each file's type-checking).
pub(crate) fn reset_global_resolution_fuel() {
    GLOBAL_RESOLUTION_FUEL.set(0);
}

/// Read the current global resolution fuel counter (for snapshot/restore).
pub(crate) fn global_resolution_fuel_value() -> u32 {
    GLOBAL_RESOLUTION_FUEL.get()
}

/// Restore the global resolution fuel counter to a previously captured value.
///
/// Used by speculative sites (return-type inference) that should not bill
/// their work against the global fuel budget when the speculation is rolled
/// back - the work will be redone in the non-speculative pass.
pub(crate) fn restore_global_resolution_fuel(value: u32) {
    GLOBAL_RESOLUTION_FUEL.set(value);
}
