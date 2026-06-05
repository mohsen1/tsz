//! Recursion guard shared by diagnostic type-display normalization passes.

/// Maximum nesting depth for the diagnostic display normalization passes.
///
/// Before a type is handed to the solver's diagnostic formatter, the checker
/// runs cosmetic normalization passes over it (resolving `Lazy` references,
/// widening fresh literals, re-applying display aliases, materializing finite
/// mapped types, stripping excess-property wrappers). These passes recurse
/// structurally through applications, unions, intersections, and object shapes,
/// and re-enter on each resolved/evaluated `Lazy` reference. On deeply
/// self-expanding generic types, that recursion never reaches a non-lazy
/// fixpoint and overflows the worker stack (issue #12455).
///
/// The downstream formatter already truncates nested type printing at depth 8
/// (`max_depth`) and elides long property-receiver objects by depth 26, so any
/// normalization performed below this bound is never observable in the rendered
/// diagnostic. Capping the passes here therefore bottoms out the recursion
/// without changing any displayed output: once the limit is reached the type is
/// returned unchanged. The bound is chosen far above the formatter's visible
/// depth so realistic diagnostics are unaffected, yet far below the thousands of
/// frames required to exhaust the worker stack.
const MAX_DIAGNOSTIC_DISPLAY_RECURSION_DEPTH: u32 = 100;

thread_local! {
    static DISPLAY_RECURSION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII guard that bounds the recursion depth of the diagnostic display
/// normalization passes (see [`MAX_DIAGNOSTIC_DISPLAY_RECURSION_DEPTH`]).
///
/// A single shared thread-local counter spans every mutually-recursive display
/// normalization function, so the total stack depth across them is bounded even
/// when one pass re-enters another. The depth is decremented on `Drop`, so every
/// return path, including early exits, is accounted for without threading a
/// depth parameter through each call site.
pub(in crate::error_reporter::core) struct DisplayRecursionGuard;

impl DisplayRecursionGuard {
    /// Enter one level of display-normalization recursion.
    ///
    /// Returns `None` once the depth cap is reached; the caller must then leave
    /// the type unchanged (return `ty` / `None`) instead of recursing further.
    pub(in crate::error_reporter::core) fn enter() -> Option<Self> {
        DISPLAY_RECURSION_DEPTH.with(|depth| {
            if depth.get() >= MAX_DIAGNOSTIC_DISPLAY_RECURSION_DEPTH {
                None
            } else {
                depth.set(depth.get() + 1);
                Some(Self)
            }
        })
    }
}

impl Drop for DisplayRecursionGuard {
    fn drop(&mut self) {
        DISPLAY_RECURSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}
