//! Re-entrant guard for the mapped-keys extraction walk.
//!
//! `extract_mapped_keys_impl` recurses through `resolve_lazy` def bodies, and
//! mutually-referential bodies (`A`'s shared-store body referencing `Lazy(B)`
//! while `B`'s references `Lazy(A)`) would otherwise recurse without progress
//! until stack overflow. Resolution forms are not guaranteed acyclic —
//! per-checker refinement and shared-store publication can produce forms that
//! point at each other — so a same-`TypeId` re-entry returns `None` (defer),
//! matching the existing "cannot extract keys" semantics.

use super::key_types::MappedKeys;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::TypeResolver;
use crate::types::TypeId;
use rustc_hash::FxHashSet;
use std::cell::RefCell;

thread_local! {
    /// `TypeId`s whose mapped-key extraction is in flight on this thread.
    ///
    /// Keys are interner-instance-local, so the set must be empty between
    /// compilations: a leaked entry would make a fresh `TypeId` reusing the
    /// same value re-enter as a false cycle and defer (`None`) for a type that
    /// genuinely has extractable keys. Membership is therefore owned by the
    /// RAII [`MappedKeysVisitGuard`], which removes the entry on drop — on the
    /// normal return path *and* when extraction unwinds via a panic that a
    /// caller (`try_tsz`, LSP) catches and swallows mid-recursion. (Before
    /// #13368 the removal was a manual post-call statement skipped on unwind,
    /// leaking the key into the next compilation on a reused worker thread.)
    static EXTRACT_MAPPED_KEYS_VISITING: RefCell<FxHashSet<TypeId>> =
        RefCell::new(FxHashSet::default());
}

/// RAII membership guard for the mapped-keys extraction walk.
///
/// [`enter`](Self::enter) returns `None` when `type_id`'s extraction is already
/// in flight on this thread (a re-entrant resolution cycle); the caller defers
/// with `None`, matching the existing "cannot extract keys" semantics.
/// Otherwise it records membership and clears it on drop, so the set is
/// restored even if `extract_mapped_keys_impl` unwinds.
#[must_use]
struct MappedKeysVisitGuard(TypeId);

impl MappedKeysVisitGuard {
    fn enter(type_id: TypeId) -> Option<Self> {
        EXTRACT_MAPPED_KEYS_VISITING.with(|visiting| {
            if visiting.borrow_mut().insert(type_id) {
                Some(Self(type_id))
            } else {
                None
            }
        })
    }
}

impl Drop for MappedKeysVisitGuard {
    fn drop(&mut self) {
        EXTRACT_MAPPED_KEYS_VISITING.with(|visiting| {
            visiting.borrow_mut().remove(&self.0);
        });
    }
}

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Extract mapped keys from a type (for mapped type iteration), guarded
    /// against re-entrant resolution cycles.
    pub(in crate::evaluation) fn extract_mapped_keys(
        &mut self,
        type_id: TypeId,
    ) -> Option<MappedKeys> {
        let _guard = MappedKeysVisitGuard::enter(type_id)?;
        self.extract_mapped_keys_impl(type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_visiting(type_id: TypeId) -> bool {
        EXTRACT_MAPPED_KEYS_VISITING.with(|visiting| visiting.borrow().contains(&type_id))
    }

    #[test]
    fn reentry_of_in_flight_type_is_rejected() {
        let t = TypeId(4242);
        let outer = MappedKeysVisitGuard::enter(t).expect("first entry succeeds");
        assert!(is_visiting(t));
        assert!(
            MappedKeysVisitGuard::enter(t).is_none(),
            "re-entering an in-flight TypeId must defer"
        );
        drop(outer);
        assert!(!is_visiting(t), "drop must restore membership");
    }

    /// #13368: the guard must clear membership even when the guarded work
    /// unwinds via a panic a caller catches, so a stale interner-local key can
    /// never leak into the next compilation on a reused worker thread.
    #[test]
    fn membership_is_restored_on_unwind() {
        let t = TypeId(99);
        let result = std::panic::catch_unwind(|| {
            let _guard = MappedKeysVisitGuard::enter(t).expect("entry succeeds");
            assert!(is_visiting(t));
            panic!("simulated mid-extraction panic");
        });
        assert!(result.is_err(), "the closure panicked");
        assert!(
            !is_visiting(t),
            "guard Drop must remove the key during unwind"
        );
    }
}
