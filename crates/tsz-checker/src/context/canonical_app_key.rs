//! Canonical key for the `type_resolution_visiting` cycle guard.

use crate::query_boundaries::common::TypeDatabase;
use tsz_solver::TypeId;
use tsz_solver::def::{DefId, DefinitionStore};

use crate::query_boundaries::state::type_environment as query;

/// Canonical key for the `type_resolution_visiting` cycle guard.
///
/// For `Application(Lazy(def), args)` types the key is
/// [`CanonicalAppKey::App`] carrying the *canonical* `DefId` (resolved through
/// the import-alias forwarding chain at check time) plus the argument vector,
/// so that all import-alias variants of the same logical generic definition
/// collapse onto one key. For every other shape the key is
/// [`CanonicalAppKey::Raw`] over the raw `TypeId`, preserving the prior
/// per-`TypeId` behavior exactly.
///
/// The canonical `DefId` is recomputed each time the key is built (it is never
/// stored on an interned `TypeId`), which keeps the guard order-independent and
/// always consistent with the current resolver generation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CanonicalAppKey {
    /// `Application(Lazy(canonical_def), args)`.
    App(DefId, Box<[TypeId]>),
    /// Any non-`Application(Lazy, _)` type, keyed by its raw `TypeId`.
    Raw(TypeId),
}

impl CanonicalAppKey {
    /// Build the cycle-guard key for `type_id`. The canonical `DefId` is
    /// resolved through the import-alias forwarding chain at call time (never
    /// stored on the interned `TypeId`), keeping the key order-independent.
    /// Decomposition uses the query-boundary helpers, not raw `TypeData`.
    pub fn build(
        db: &dyn TypeDatabase,
        definition_store: &DefinitionStore,
        type_id: TypeId,
    ) -> Self {
        if let Some((base, args)) = query::application_info(db, type_id)
            && let Some(def_id) = query::lazy_def_id(db, base)
        {
            let canonical = definition_store.canonical_def_id(def_id);
            return Self::App(canonical, args.into_boxed_slice());
        }
        Self::Raw(type_id)
    }
}
