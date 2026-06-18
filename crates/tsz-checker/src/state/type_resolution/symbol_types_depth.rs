//! Recursion depth guard for symbol type-reference resolution.

/// Maximum nesting depth for `type_reference_symbol_type_with_params`'s
/// alias-forwarding recursion. A mutually-aliasing pair produced by a
/// raw-`SymbolId` cross-file collision (`Dataset` <-> `OutputDataset`) would
/// otherwise ping-pong through the recursion until the stack overflows and
/// aborts the compile. The cap is far above any legitimate alias chain and far
/// below stack exhaustion, so valid code is unaffected. Refs #13212.
const MAX_TYPE_REFERENCE_RESOLUTION_DEPTH: u32 = 350;

thread_local! {
    static TYPE_REFERENCE_RESOLUTION_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII depth counter for
/// `CheckerState::type_reference_symbol_type_with_params`.
pub(super) struct TypeReferenceResolutionDepthGuard;

impl TypeReferenceResolutionDepthGuard {
    /// Enters one recursion level; returns `None` once the depth cap is hit.
    pub(super) fn enter() -> Option<Self> {
        TYPE_REFERENCE_RESOLUTION_DEPTH.with(|depth| {
            if depth.get() >= MAX_TYPE_REFERENCE_RESOLUTION_DEPTH {
                None
            } else {
                depth.set(depth.get() + 1);
                Some(Self)
            }
        })
    }
}

impl Drop for TypeReferenceResolutionDepthGuard {
    fn drop(&mut self) {
        TYPE_REFERENCE_RESOLUTION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}
