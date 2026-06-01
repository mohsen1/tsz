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

#[test]
fn redeclaration_identity_relation_does_not_reuse_assignability_cache_slot() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let policy = RelationPolicy::default();
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let source = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let target = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let assignability_key =
        RelationCacheKey::for_assignability(source, target, policy.cache_config());
    let identical_key = RelationCacheKey::for_identical(source, target, policy.cache_config());

    assert_ne!(
        assignability_key, identical_key,
        "redeclaration identity and assignability must occupy distinct relation cache keys",
    );

    let assignability_cached = db.is_assignable_to_with_policy(source, target, policy);
    assert!(
        assignability_cached,
        "ordinary structural assignability should allow extra source properties",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        Some(assignability_cached),
        "ordinary assignability should populate the assignability cache slot",
    );

    let redeclaration_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let redeclaration_with_cache_context = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext {
            query_db: Some(&db),
            ..RelationContext::default()
        },
    )
    .is_related();

    assert!(
        !redeclaration_uncached,
        "redeclaration identity must reject structurally assignable but non-identical object types",
    );
    assert_eq!(
        redeclaration_with_cache_context, redeclaration_uncached,
        "redeclaration identity with a query cache context must match uncached identity semantics",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        Some(assignability_cached),
        "redeclaration identity must not overwrite or consume the ordinary assignability slot",
    );
}

#[test]
fn redeclaration_identity_relation_does_not_reuse_subtype_cache_slot() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let policy = RelationPolicy::default();
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let source = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let target = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let subtype_key = RelationCacheKey::for_subtype(source, target, policy.cache_config());
    let identical_key = RelationCacheKey::for_identical(source, target, policy.cache_config());

    assert_ne!(
        subtype_key, identical_key,
        "redeclaration identity and subtype must occupy distinct relation cache keys",
    );

    let subtype_cached = db.is_subtype_of_with_policy(source, target, policy);
    assert!(
        subtype_cached,
        "ordinary structural subtype should allow extra source properties",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        Some(subtype_cached),
        "ordinary subtype should populate the subtype cache slot",
    );

    let redeclaration_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let redeclaration_with_cache_context = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext {
            query_db: Some(&db),
            ..RelationContext::default()
        },
    )
    .is_related();

    assert!(
        !redeclaration_uncached,
        "redeclaration identity must reject structurally subtype-compatible but non-identical object types",
    );
    assert_eq!(
        redeclaration_with_cache_context, redeclaration_uncached,
        "redeclaration identity with a query cache context must match uncached identity semantics",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        Some(subtype_cached),
        "redeclaration identity must not overwrite or consume the ordinary subtype slot",
    );
}

#[test]
fn redeclaration_identity_relation_does_not_populate_ordinary_relation_cache_slots() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let policy = RelationPolicy::default();
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");
    let source = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let target = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let assignability_key =
        RelationCacheKey::for_assignability(source, target, policy.cache_config());
    let subtype_key = RelationCacheKey::for_subtype(source, target, policy.cache_config());
    let identical_key = RelationCacheKey::for_identical(source, target, policy.cache_config());

    assert_ne!(
        assignability_key, identical_key,
        "redeclaration identity and assignability must occupy distinct relation cache keys",
    );
    assert_ne!(
        subtype_key, identical_key,
        "redeclaration identity and subtype must occupy distinct relation cache keys",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        None,
        "assignability slot should start empty",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        None,
        "subtype slot should start empty",
    );

    let redeclaration_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let redeclaration_with_cache_context = query_relation(
        &interner,
        source,
        target,
        RelationKind::RedeclarationIdentical,
        policy,
        RelationContext {
            query_db: Some(&db),
            ..RelationContext::default()
        },
    )
    .is_related();

    assert!(
        !redeclaration_uncached,
        "redeclaration identity must reject structurally compatible but non-identical object types",
    );
    assert_eq!(
        redeclaration_with_cache_context, redeclaration_uncached,
        "redeclaration identity with a query cache context must match uncached identity semantics",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        None,
        "redeclaration identity must not populate the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        None,
        "redeclaration identity must not populate the ordinary subtype slot",
    );
}
