//! Relation cache-key construction for the assignability boundary.
//!
//! Submodule keeps the assignability boundary under its LOC ceiling while the
//! boundary still owns cache-key policy.

use tsz_solver::TypeId;

use super::relation_policy;

/// Boundary-safe flag constants for relation policy.
///
/// Mirrors the solver's typed `RelationFlags` bit surface while keeping the
/// checker-facing packed `u16` protocol quarantined to this boundary. Checker
/// code should use these constants when constructing relation policy flags
/// (e.g., in `pack_relation_flags`).
pub(crate) struct RelationFlags;

impl RelationFlags {
    pub const STRICT_NULL_CHECKS: u16 = tsz_solver::RelationFlags::STRICT_NULL_CHECKS.bits() as u16;
    pub const STRICT_FUNCTION_TYPES: u16 =
        tsz_solver::RelationFlags::STRICT_FUNCTION_TYPES.bits() as u16;
    pub const EXACT_OPTIONAL_PROPERTY_TYPES: u16 =
        tsz_solver::RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES.bits() as u16;
    pub const NO_UNCHECKED_INDEXED_ACCESS: u16 =
        tsz_solver::RelationFlags::NO_UNCHECKED_INDEXED_ACCESS.bits() as u16;
    pub const NO_ERASE_GENERICS: u16 = tsz_solver::RelationFlags::NO_ERASE_GENERICS.bits() as u16;
    pub const ALLOW_BIVARIANT_REST: u16 =
        tsz_solver::RelationFlags::ALLOW_BIVARIANT_REST.bits() as u16;
    pub const ALLOW_ERASED_GENERIC_SIGNATURE_RETRY: u16 =
        tsz_solver::RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY.bits() as u16;
    pub const DISABLE_METHOD_BIVARIANCE: u16 =
        tsz_solver::RelationFlags::DISABLE_METHOD_BIVARIANCE.bits() as u16;
}

/// Re-export of the solver's relation cache key type.
///
/// Used by the assignability checker to construct cache keys for memoizing
/// subtype and assignability relation results.
pub(crate) use tsz_solver::RelationCacheKey;

/// Build a cache key for an assignability lookup.
///
/// Canonical, typed construction point for assignability cache keys in the
/// checker. Callers pass a packed `u16` from `pack_relation_flags()` and
/// this helper funnels it through the solver's typed `RelationCacheConfig`,
/// so no call site needs to hand-roll the key's internal representation.
///
/// The resulting config is produced by the solver's typed `RelationPolicy`
/// bridge, so this write path lands in the same cache slot as the solver's
/// internal write path.
pub(crate) const fn assignability_cache_key(
    source: TypeId,
    target: TypeId,
    flags: u16,
) -> RelationCacheKey {
    RelationCacheKey::for_assignability(
        source,
        target,
        relation_policy::from_checker_flags_u16(flags).cache_config(),
    )
}

/// Build a cache key for a subtype lookup. See [`assignability_cache_key`].
pub(crate) const fn subtype_cache_key(
    source: TypeId,
    target: TypeId,
    flags: u16,
) -> RelationCacheKey {
    RelationCacheKey::for_subtype(
        source,
        target,
        relation_policy::from_checker_flags_u16(flags).cache_config(),
    )
}
