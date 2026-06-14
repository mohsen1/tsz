//! Lock-poison recovery policy for the solver's internal synchronization
//! primitives.
//!
//! The solver guards a handful of internal `RwLock`/`Mutex` fields (the type
//! interner shards, the slice interner's `items` vector, the definition
//! store's append-only symbol-mappings log and its snapshots). A poisoned lock
//! means another thread panicked while holding it — the protected state is in
//! an unknown, unrecoverable condition, so the only correct policy is to
//! propagate the panic rather than silently observe torn state.
//!
//! Historically each call site inlined `.expect("... lock poisoned")` with its
//! own wording, so the recover-by-panic policy was copy-pasted with four
//! different messages across 14 sites. Funnelling every acquisition through
//! these extension traits gives one uniform panic message keyed off the lock
//! name and makes a future synchronization change (e.g. switching to
//! `parking_lot`, or to `PoisonError::into_inner` recovery) a one-line edit
//! instead of a crate-wide sweep.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Single source of truth for the "an internal solver lock was poisoned"
/// panic. Cold and never inlined so the acquisition fast paths stay lean.
#[cold]
#[inline(never)]
fn lock_poisoned(lock_name: &str) -> ! {
    panic!("solver lock poisoned: {lock_name}");
}

/// Acquire an internal [`RwLock`] whose poisoning is treated as unrecoverable.
///
/// Both methods panic with a uniform message keyed off `lock_name` when the
/// lock is poisoned, preserving the original panic-on-poison semantics.
pub(crate) trait RwLockExt<T: ?Sized> {
    /// Acquire a shared read guard, panicking uniformly on poison.
    fn read_unpoisoned(&self, lock_name: &'static str) -> RwLockReadGuard<'_, T>;
    /// Acquire an exclusive write guard, panicking uniformly on poison.
    fn write_unpoisoned(&self, lock_name: &'static str) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    #[inline]
    fn read_unpoisoned(&self, lock_name: &'static str) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|_| lock_poisoned(lock_name))
    }

    #[inline]
    fn write_unpoisoned(&self, lock_name: &'static str) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|_| lock_poisoned(lock_name))
    }
}

/// Acquire an internal [`Mutex`] whose poisoning is treated as unrecoverable.
pub(crate) trait MutexExt<T: ?Sized> {
    /// Acquire the mutex guard, panicking uniformly on poison.
    fn lock_unpoisoned(&self, lock_name: &'static str) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> MutexExt<T> for Mutex<T> {
    #[inline]
    fn lock_unpoisoned(&self, lock_name: &'static str) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|_| lock_poisoned(lock_name))
    }
}
