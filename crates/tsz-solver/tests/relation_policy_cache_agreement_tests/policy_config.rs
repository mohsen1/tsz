//! Relation-policy construction coherence tests for relation cache configs.

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
fn relation_policy_cache_config_unifies_equivalent_flag_and_builder_bits() {
    let cases = [
        (
            "strict subtype checking",
            RelationPolicy::from_relation_flags(RelationFlags::STRICT_SUBTYPE_CHECKING),
            RelationPolicy::unflagged_compatibility().with_strict_subtype_checking(true),
        ),
        (
            "strict any propagation",
            RelationPolicy::from_relation_flags(RelationFlags::STRICT_ANY_PROPAGATION),
            RelationPolicy::unflagged_compatibility().with_strict_any_propagation(true),
        ),
        (
            "skip weak type checks",
            RelationPolicy::from_relation_flags(RelationFlags::SKIP_WEAK_TYPE_CHECKS),
            RelationPolicy::unflagged_compatibility().with_skip_weak_type_checks(true),
        ),
        (
            "disable generic erasure",
            RelationPolicy::from_relation_flags(RelationFlags::NO_ERASE_GENERICS),
            RelationPolicy::unflagged_compatibility().with_erase_generics(false),
        ),
    ];

    for (name, flagged, builder) in cases {
        assert_eq!(
            flagged.cache_config(),
            builder.cache_config(),
            "{name} must produce the same cache config through flags and builders",
        );
    }
}

#[test]
fn assignability_cache_reuses_slot_for_flag_and_builder_strict_subtype_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let run = interner.intern_string("run");
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);
    let dog_method = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(dog)],
        TypeId::VOID,
    ));
    let animal_method = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(animal)],
        TypeId::VOID,
    ));
    let source = interner.object(vec![PropertyInfo::method(run, dog_method)]);
    let target = interner.object(vec![PropertyInfo::method(run, animal_method)]);

    let base_flags = RelationFlags::STRICT_FUNCTION_TYPES;
    let flagged =
        RelationPolicy::from_relation_flags(base_flags | RelationFlags::STRICT_SUBTYPE_CHECKING);
    let builder =
        RelationPolicy::from_relation_flags(base_flags).with_strict_subtype_checking(true);
    let key = RelationCacheKey::for_assignability(source, target, flagged.cache_config());

    assert_eq!(
        flagged.cache_config(),
        builder.cache_config(),
        "flagged and builder strict-subtype policies must share one cache config",
    );
    assert_eq!(
        key,
        RelationCacheKey::for_assignability(source, target, builder.cache_config()),
        "equivalent policy construction must share the assignability cache slot",
    );

    let flagged_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        flagged,
        RelationContext::default(),
    )
    .is_related();
    let builder_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        builder,
        RelationContext::default(),
    )
    .is_related();

    assert_eq!(
        flagged_uncached, builder_uncached,
        "equivalent strict-subtype policies must agree before cache lookup",
    );
    assert!(
        !flagged_uncached,
        "strict subtype checking should disable method bivariance for assignability",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, flagged),
        flagged_uncached,
        "flagged strict-subtype policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(key),
        Some(flagged_uncached),
        "flagged strict-subtype result must populate the shared slot",
    );

    db.reset_relation_cache_stats();
    assert_eq!(
        db.is_assignable_to_with_policy(source, target, builder),
        builder_uncached,
        "builder strict-subtype policy must reuse the equivalent cached answer",
    );
    let stats = db.relation_cache_stats();
    assert!(
        stats.assignability_hits >= 1,
        "builder strict-subtype lookup should hit the flagged policy slot",
    );
    assert_eq!(
        stats.assignability_misses, 0,
        "equivalent builder policy lookup should not miss after flagged policy populated the slot",
    );
}
