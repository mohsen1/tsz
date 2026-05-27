//! Cache-enabled/cache-disabled agreement tests for behavior-changing relation
//! policies.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{
    FunctionShape, ParamInfo, PropertyInfo, RelationCacheKey, RelationFlags, TypeId,
};

#[test]
fn subtype_cache_any_propagation_mode_matches_uncached_nested_any() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::OBJECT)]);
    let all_policy = RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::All);
    let top_level_only_policy =
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);

    let all_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        all_policy,
        RelationContext::default(),
    );
    let top_level_only_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        top_level_only_policy,
        RelationContext::default(),
    );

    let all_cached = db.is_subtype_of_with_policy(source, target, all_policy);
    let top_level_only_cached = db.is_subtype_of_with_policy(source, target, top_level_only_policy);
    let top_level_only_cached_again =
        db.is_subtype_of_with_policy(source, target, top_level_only_policy);
    let top_level_uncached = query_relation(
        &interner,
        TypeId::ANY,
        TypeId::OBJECT,
        RelationKind::Subtype,
        top_level_only_policy,
        RelationContext::default(),
    );
    let top_level_cached =
        db.is_subtype_of_with_policy(TypeId::ANY, TypeId::OBJECT, top_level_only_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        all_cached,
        all_uncached.is_related(),
        "cached subtype must match uncached all-depth any propagation",
    );
    assert_eq!(
        top_level_only_cached,
        top_level_only_uncached.is_related(),
        "cached subtype must match uncached top-level-only any propagation",
    );
    assert_eq!(
        top_level_only_cached_again, top_level_only_cached,
        "second top-level-only lookup should reuse the policy-shaped answer",
    );
    assert!(
        all_cached,
        "`AnyPropagationMode::All` should allow nested `any` to satisfy `object`",
    );
    assert!(
        !top_level_only_cached,
        "top-level-only any propagation should not allow nested `any` to satisfy `object`",
    );
    assert_eq!(
        top_level_cached,
        top_level_uncached.is_related(),
        "cached subtype must match uncached top-level `any` comparison",
    );
    assert!(
        top_level_cached,
        "top-level-only any propagation should still allow top-level `any` to satisfy `object`",
    );
    assert!(
        stats.subtype_hits >= 1,
        "second top-level-only lookup should hit the subtype cache",
    );
    assert!(
        stats.subtype_misses >= 2,
        "all-depth and top-level-only policies should miss in separate cache slots",
    );
}

#[test]
fn assignability_cache_strict_function_types_matches_uncached_function_variance() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let handler_dog = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(dog)],
        TypeId::VOID,
    ));
    let handler_animal = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(animal)],
        TypeId::VOID,
    ));

    let strict_policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES);
    let loose_policy = RelationPolicy::from_flags(0);

    let strict_uncached = query_relation(
        &interner,
        handler_dog,
        handler_animal,
        RelationKind::Assignable,
        strict_policy,
        RelationContext::default(),
    );
    let loose_uncached = query_relation(
        &interner,
        handler_dog,
        handler_animal,
        RelationKind::Assignable,
        loose_policy,
        RelationContext::default(),
    );

    let strict_cached = db.is_assignable_to_with_policy(handler_dog, handler_animal, strict_policy);
    let loose_cached = db.is_assignable_to_with_policy(handler_dog, handler_animal, loose_policy);
    let strict_cached_again =
        db.is_assignable_to_with_policy(handler_dog, handler_animal, strict_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        strict_cached,
        strict_uncached.is_related(),
        "cached strict function variance must match the uncached relation facade",
    );
    assert_eq!(
        loose_cached,
        loose_uncached.is_related(),
        "cached loose function variance must match the uncached relation facade",
    );
    assert_eq!(
        strict_cached_again, strict_cached,
        "second strict function variance lookup should reuse the policy-shaped answer",
    );
    assert!(
        !strict_cached,
        "strict function parameter variance should reject `(dog) => void` where `(animal) => void` is required",
    );
    assert!(
        loose_cached,
        "loose function parameter variance should accept the bivariant parameter comparison",
    );
    assert!(
        stats.assignability_hits >= 1,
        "second strict lookup should hit the assignability cache",
    );
    assert!(
        stats.assignability_misses >= 2,
        "strict and loose variance policies should miss in separate cache slots",
    );
}

#[test]
fn subtype_cache_allow_void_return_matches_uncached_function_return_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(vec![], TypeId::STRING));
    let target = interner.function(FunctionShape::new(vec![], TypeId::VOID));

    let strict_policy = RelationPolicy::from_flags(0);
    let allow_void_policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_ALLOW_VOID_RETURN);

    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        strict_policy,
        RelationContext::default(),
    );
    let allow_void_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        allow_void_policy,
        RelationContext::default(),
    );

    let strict_cached = db.is_subtype_of_with_policy(source, target, strict_policy);
    let allow_void_cached = db.is_subtype_of_with_policy(source, target, allow_void_policy);
    let strict_cached_again = db.is_subtype_of_with_policy(source, target, strict_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        strict_cached,
        strict_uncached.is_related(),
        "cached strict return subtype must match the uncached relation facade",
    );
    assert_eq!(
        allow_void_cached,
        allow_void_uncached.is_related(),
        "cached allow-void return subtype must match the uncached relation facade",
    );
    assert_eq!(
        strict_cached_again, strict_cached,
        "second strict return lookup should reuse the policy-shaped answer",
    );
    assert!(
        !strict_cached,
        "strict return compatibility should reject `() => string` where `() => void` is required",
    );
    assert!(
        allow_void_cached,
        "allow-void return compatibility should accept ignored source return values",
    );
    assert!(
        stats.subtype_hits >= 1,
        "second strict return lookup should hit the subtype cache",
    );
    assert!(
        stats.subtype_misses >= 2,
        "strict and allow-void return policies should miss in separate cache slots",
    );
}

#[test]
fn assignability_cache_exact_optional_matches_uncached_property_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");

    let source = interner.object(vec![PropertyInfo::new(x, TypeId::UNDEFINED)]);
    let target = interner.object(vec![PropertyInfo::opt(x, TypeId::NUMBER)]);

    let inexact_policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);
    let exact_policy = RelationPolicy::from_flags(
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS
            | RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES,
    );

    let inexact_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        inexact_policy,
        RelationContext::default(),
    );
    let exact_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        exact_policy,
        RelationContext::default(),
    );

    let inexact_cached = db.is_assignable_to_with_policy(source, target, inexact_policy);
    let exact_cached = db.is_assignable_to_with_policy(source, target, exact_policy);
    let inexact_cached_again = db.is_assignable_to_with_policy(source, target, inexact_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        inexact_cached,
        inexact_uncached.is_related(),
        "cached inexact optional-property assignability must match the uncached relation facade",
    );
    assert_eq!(
        exact_cached,
        exact_uncached.is_related(),
        "cached exact optional-property assignability must match the uncached relation facade",
    );
    assert_eq!(
        inexact_cached_again, inexact_cached,
        "second inexact optional-property lookup should reuse the policy-shaped answer",
    );
    assert!(
        inexact_cached,
        "inexact optional-property mode should allow explicit `undefined` for an optional property",
    );
    assert!(
        !exact_cached,
        "exact optional-property mode should reject explicit `undefined` for an optional property",
    );
    assert!(
        stats.assignability_hits >= 1,
        "second inexact optional-property lookup should hit the assignability cache",
    );
    assert!(
        stats.assignability_misses >= 2,
        "exact and inexact optional-property policies should miss in separate cache slots",
    );
}

#[test]
fn subtype_cache_strict_readonly_identity_matches_uncached_property_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");

    let source = interner.object(vec![PropertyInfo::readonly(x, TypeId::NUMBER)]);
    let target = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    let permissive_policy = RelationPolicy::from_flags(0);
    let strict_policy =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_READONLY_IDENTITY);

    let permissive_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        permissive_policy,
        RelationContext::default(),
    );
    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        strict_policy,
        RelationContext::default(),
    );

    let permissive_cached = db.is_subtype_of_with_policy(source, target, permissive_policy);
    let strict_cached = db.is_subtype_of_with_policy(source, target, strict_policy);
    let permissive_cached_again = db.is_subtype_of_with_policy(source, target, permissive_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        permissive_cached,
        permissive_uncached.is_related(),
        "cached permissive readonly subtype must match the uncached relation facade",
    );
    assert_eq!(
        strict_cached,
        strict_uncached.is_related(),
        "cached strict readonly subtype must match the uncached relation facade",
    );
    assert_eq!(
        permissive_cached_again, permissive_cached,
        "second permissive readonly lookup should reuse the policy-shaped answer",
    );
    assert!(
        permissive_cached,
        "permissive readonly policy should allow a readonly property to satisfy a mutable target",
    );
    assert!(
        !strict_cached,
        "strict readonly identity should reject readonly-to-mutable property comparison",
    );
    assert!(
        stats.subtype_hits >= 1,
        "second permissive readonly lookup should hit the subtype cache",
    );
    assert!(
        stats.subtype_misses >= 2,
        "strict and permissive readonly policies should miss in separate cache slots",
    );
}

#[test]
fn assignability_cache_disable_method_bivariance_matches_uncached_method_policy() {
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

    let bivariant_policy =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let sound_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::DISABLE_METHOD_BIVARIANCE,
    );

    let bivariant_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_policy,
        RelationContext::default(),
    );
    let sound_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        sound_policy,
        RelationContext::default(),
    );

    let bivariant_cached = db.is_assignable_to_with_policy(source, target, bivariant_policy);
    let sound_cached = db.is_assignable_to_with_policy(source, target, sound_policy);
    let bivariant_cached_again = db.is_assignable_to_with_policy(source, target, bivariant_policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        bivariant_cached,
        bivariant_uncached.is_related(),
        "cached method-bivariant assignability must match the uncached relation facade",
    );
    assert_eq!(
        sound_cached,
        sound_uncached.is_related(),
        "cached sound method assignability must match the uncached relation facade",
    );
    assert_eq!(
        bivariant_cached_again, bivariant_cached,
        "second method-bivariant lookup should reuse the policy-shaped answer",
    );
    assert!(
        bivariant_cached,
        "strict function types should still allow method parameter bivariance by default",
    );
    assert!(
        !sound_cached,
        "disabling method bivariance should reject `(dog) => void` where `(animal) => void` is required",
    );
    assert!(
        stats.assignability_hits >= 1,
        "second method-bivariant lookup should hit the assignability cache",
    );
    assert!(
        stats.assignability_misses >= 2,
        "method-bivariant and sound-method policies should miss in separate cache slots",
    );
}

#[test]
fn assignability_cache_strict_subtype_checking_matches_uncached_method_policy() {
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

    let ordinary = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES)
        .with_strict_subtype_checking(false);
    let strict = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES)
        .with_strict_subtype_checking(true);
    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let strict_key = RelationCacheKey::for_assignability(source, target, strict.cache_config());

    assert_ne!(
        ordinary_key, strict_key,
        "ordinary and strict-subtype policies must occupy distinct assignability cache slots",
    );

    let ordinary_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        ordinary,
        RelationContext::default(),
    )
    .is_related();
    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary strict-function assignability should keep method parameters bivariant",
    );
    assert!(
        !strict_uncached,
        "strict subtype checking should disable method bivariance for assignability",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary result must be stored in the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        None,
        "strict-subtype lookup must not hit the ordinary slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict),
        strict_uncached,
        "cached strict-subtype policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_uncached),
        "strict-subtype result must be stored in the strict assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary assignability slot must remain intact after the strict lookup",
    );
}
