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

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Extract mapped keys from a type (for mapped type iteration), guarded
    /// against re-entrant resolution cycles.
    pub(in crate::evaluation) fn extract_mapped_keys(
        &mut self,
        type_id: TypeId,
    ) -> Option<MappedKeys> {
        thread_local! {
            static EXTRACT_MAPPED_KEYS_VISITING: std::cell::RefCell<FxHashSet<u32>> =
                std::cell::RefCell::new(FxHashSet::default());
        }
        let entered =
            EXTRACT_MAPPED_KEYS_VISITING.with(|visiting| visiting.borrow_mut().insert(type_id.0));
        if !entered {
            return None;
        }
        let result = self.extract_mapped_keys_impl(type_id);
        EXTRACT_MAPPED_KEYS_VISITING.with(|visiting| {
            visiting.borrow_mut().remove(&type_id.0);
        });
        result
    }
}
