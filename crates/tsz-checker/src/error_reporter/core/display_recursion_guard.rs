//! Recursion bound for the type-display normalizers in the error reporter.
//!
//! Several normalizers walk a type's structure while *formatting a diagnostic*
//! (resolving `Lazy` aliases, widening fresh literals). On self-referential
//! type graphs that walk can grow without bound — each `Lazy` resolution may
//! re-intern a fresh `Application` — and overflow the worker thread's native
//! stack even though the type checking itself already terminated. tsc bounds
//! type-display nesting; tsz must too. [`DisplayRecursionGuard`] provides that
//! bound as a small RAII helper shared by the affected normalizers.

use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

/// Maximum nesting depth for the recursive type-display normalizers guarded by
/// [`DisplayRecursionGuard`].
///
/// The cap is a backstop: the primary terminator is the path-scoped cycle set,
/// which stops the moment a `TypeId` already on the current expansion path is
/// re-entered. Most self-referential graphs (e.g. repeated `Atom<unknown>` /
/// `StoreApi<...>` nodes) re-use identical `TypeId`s, so the cycle set both
/// prevents the overflow *and* keeps display formatting cheap by not re-walking
/// the same node. The cap sits far above any realistic display nesting (real
/// diagnostics are only a handful of levels deep) and far below the depth that
/// exhausts the worker thread's stack, so ordinary diagnostics are unchanged.
const MAX_DISPLAY_NORMALIZE_DEPTH: u32 = 100;

thread_local! {
    static DISPLAY_NORMALIZE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static DISPLAY_NORMALIZE_VISITING: std::cell::RefCell<FxHashSet<TypeId>> =
        std::cell::RefCell::new(FxHashSet::default());
}

/// RAII guard bounding the recursive type-display normalizers.
///
/// [`enter`](Self::enter) returns `None` — telling the caller to leave the type
/// un-normalized — when either the type is already being expanded on the
/// current path (a cycle) or the recursion has reached
/// [`MAX_DISPLAY_NORMALIZE_DEPTH`]. Otherwise it returns a guard that, on drop,
/// pops the type from the path set and decrements the depth, so every early
/// return is balanced automatically. The path set is path-scoped, so any
/// `TypeId` it holds is genuinely an ancestor on the current call stack.
pub(in crate::error_reporter) struct DisplayRecursionGuard {
    ty: TypeId,
}

impl DisplayRecursionGuard {
    #[inline]
    pub(in crate::error_reporter) fn enter(ty: TypeId) -> Option<Self> {
        DISPLAY_NORMALIZE_DEPTH.with(|depth_cell| {
            let depth = depth_cell.get();
            if depth >= MAX_DISPLAY_NORMALIZE_DEPTH {
                return None;
            }
            // Path-scoped cycle detection: `insert` returns false when `ty` is
            // already an ancestor on the current expansion path. Sibling re-use
            // of the same node is fine — the guard removes `ty` again on drop —
            // so only genuine ancestor cycles are short-circuited.
            if !DISPLAY_NORMALIZE_VISITING.with(|set| set.borrow_mut().insert(ty)) {
                return None;
            }
            depth_cell.set(depth + 1);
            Some(Self { ty })
        })
    }
}

impl Drop for DisplayRecursionGuard {
    #[inline]
    fn drop(&mut self) {
        DISPLAY_NORMALIZE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        DISPLAY_NORMALIZE_VISITING.with(|set| {
            set.borrow_mut().remove(&self.ty);
        });
    }
}
