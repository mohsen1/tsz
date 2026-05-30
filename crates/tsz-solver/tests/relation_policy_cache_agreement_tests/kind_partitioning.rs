//! Relation-kind partitioning tests for relation caches.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::types::{PropertyInfo, RelationCacheKey, TypeId};

#[test]
fn relation_policy_cache_relation_kind_partitions_assignability_and_subtype() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let unrelated = interner.intern_string("unrelated");
    let optional = interner.intern_string("optional");

    let source = interner.object(vec![PropertyInfo::new(unrelated, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::opt(optional, TypeId::NUMBER)]);
    let policy = RelationPolicy::default();
    let assignability_key =
        RelationCacheKey::for_assignability(source, target, policy.cache_config());
    let subtype_key = RelationCacheKey::for_subtype(source, target, policy.cache_config());

    assert_ne!(
        assignability_key, subtype_key,
        "assignability and subtype must occupy distinct relation cache keys",
    );

    let assignability_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let subtype_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !assignability_uncached,
        "assignability should reject unrelated source properties against a weak target",
    );
    assert!(
        subtype_uncached,
        "structural subtype should allow the same source when the target property is optional",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, policy),
        assignability_uncached,
        "cached assignability must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        Some(assignability_uncached),
        "assignability result must be stored in the assignability cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        None,
        "subtype lookup must not hit the assignability cache slot",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(source, target, policy),
        subtype_uncached,
        "cached subtype must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        Some(subtype_uncached),
        "subtype result must be stored in the subtype cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        Some(assignability_uncached),
        "assignability slot must remain intact after the subtype lookup",
    );
}
