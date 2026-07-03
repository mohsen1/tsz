//! Relation cache-key construction for the assignability boundary.
//!
//! Submodule keeps the assignability boundary under its LOC ceiling while the
//! boundary still owns cache-key policy.

use tsz_solver::TypeId;
use tsz_solver::classes::inheritance::InheritanceGraph;
use tsz_solver::relations::relation_queries::RelationPolicy;

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
const fn with_relation_context(
    key: RelationCacheKey,
    resolver_generation: u64,
    inheritance_graph: &InheritanceGraph,
) -> RelationCacheKey {
    key.with_resolver_generation(resolver_generation)
        .with_inheritance_graph_context(
            inheritance_graph.identity(),
            inheritance_graph.generation(),
        )
}

pub(crate) const fn assignability_cache_key_for_policy(
    source: TypeId,
    target: TypeId,
    policy: RelationPolicy,
    resolver_generation: u64,
    inheritance_graph: &InheritanceGraph,
) -> RelationCacheKey {
    with_relation_context(
        RelationCacheKey::for_assignability(source, target, policy.cache_config()),
        resolver_generation,
        inheritance_graph,
    )
}

pub(crate) const fn assignability_cache_key(
    source: TypeId,
    target: TypeId,
    flags: u16,
    resolver_generation: u64,
    inheritance_graph: &InheritanceGraph,
) -> RelationCacheKey {
    assignability_cache_key_for_policy(
        source,
        target,
        relation_policy::from_checker_flags_u16(flags),
        resolver_generation,
        inheritance_graph,
    )
}

pub(crate) const fn checker_final_assignability_cache_key(
    source: TypeId,
    target: TypeId,
    flags: u16,
    resolver_generation: u64,
    inheritance_graph: &InheritanceGraph,
) -> RelationCacheKey {
    with_relation_context(
        RelationCacheKey::for_checker_assignability(
            source,
            target,
            relation_policy::from_checker_flags_u16(flags).cache_config(),
        ),
        resolver_generation,
        inheritance_graph,
    )
}

/// Build a cache key for a subtype lookup. See [`assignability_cache_key`].
pub(crate) const fn subtype_cache_key(
    source: TypeId,
    target: TypeId,
    flags: u16,
    resolver_generation: u64,
    inheritance_graph: &InheritanceGraph,
) -> RelationCacheKey {
    with_relation_context(
        RelationCacheKey::for_subtype(
            source,
            target,
            relation_policy::from_checker_flags_u16(flags).cache_config(),
        ),
        resolver_generation,
        inheritance_graph,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::SymbolId;

    #[test]
    fn checker_relation_cache_keys_partition_by_inheritance_graph_generation() {
        let graph = InheritanceGraph::new();
        let before_assignability =
            assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);
        let before_final =
            checker_final_assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);
        let before_subtype = subtype_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);

        assert_eq!(before_assignability.inheritance_graph_id, graph.identity());
        assert_eq!(before_final.inheritance_graph_id, graph.identity());
        assert_eq!(before_subtype.inheritance_graph_id, graph.identity());
        assert_eq!(before_assignability.resolver_generation, 11);
        assert_eq!(
            before_assignability.inheritance_graph_generation,
            graph.generation()
        );

        graph.add_inheritance(SymbolId(1), &[SymbolId(2)]);

        let after_assignability =
            assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);
        let after_final =
            checker_final_assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);
        let after_subtype = subtype_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 11, &graph);

        assert_eq!(after_assignability.inheritance_graph_id, graph.identity());
        assert_ne!(before_assignability, after_assignability);
        assert_ne!(before_final, after_final);
        assert_ne!(before_subtype, after_subtype);

        let after_resolver = assignability_cache_key(TypeId::STRING, TypeId::NUMBER, 0, 12, &graph);
        assert_ne!(after_assignability, after_resolver);
    }
}
