//! Callback-relation cache partitioning tests.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, RelationCacheKey, RelationFlags, TypeId,
};

#[test]
fn bivariant_callback_kind_does_not_populate_strict_assignability_slot() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let source = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(dog)],
        TypeId::VOID,
    ));
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(animal)],
        TypeId::VOID,
    ));

    let strict_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::STRICT_FUNCTION_TYPES,
    );
    let strict_assignability_key =
        RelationCacheKey::for_assignability(source, target, strict_policy.cache_config());

    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_policy,
        RelationContext::default(),
    )
    .is_related();
    assert!(
        !strict_uncached,
        "strict assignability should reject contravariant callback parameter narrowing",
    );

    let bivariant_with_cache = query_relation(
        &interner,
        source,
        target,
        RelationKind::AssignableBivariantCallbacks,
        strict_policy,
        RelationContext {
            query_db: Some(&db),
            ..RelationContext::default()
        },
    )
    .is_related();
    assert!(
        bivariant_with_cache,
        "callback-bivariant relation should accept the same parameter comparison",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_assignability_key),
        None,
        "callback mode must not populate the ordinary strict assignability slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict_policy),
        strict_uncached,
        "ordinary strict assignability must still match direct query_relation after callback mode",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_assignability_key),
        Some(strict_uncached),
        "ordinary strict assignability should populate its own cache slot after lookup",
    );
}

#[test]
fn bivariant_callback_kind_does_not_reuse_strict_assignability_slot() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let source = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(dog)],
        TypeId::VOID,
    ));
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(animal)],
        TypeId::VOID,
    ));

    let strict_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::STRICT_FUNCTION_TYPES,
    );
    let strict_assignability_key =
        RelationCacheKey::for_assignability(source, target, strict_policy.cache_config());

    let strict_cached = db.is_assignable_to_with_policy(source, target, strict_policy);
    assert!(
        !strict_cached,
        "ordinary strict assignability should reject contravariant callback parameter narrowing",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_assignability_key),
        Some(strict_cached),
        "ordinary strict assignability should populate its own cache slot first",
    );

    let bivariant_with_cache = query_relation(
        &interner,
        source,
        target,
        RelationKind::AssignableBivariantCallbacks,
        strict_policy,
        RelationContext {
            query_db: Some(&db),
            ..RelationContext::default()
        },
    )
    .is_related();
    assert!(
        bivariant_with_cache,
        "callback-bivariant relation must not reuse the ordinary strict assignability answer",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_assignability_key),
        Some(strict_cached),
        "callback-bivariant relation must leave the ordinary strict assignability slot intact",
    );
}
