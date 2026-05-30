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
    CallSignature, CallableShape, FunctionShape, ParamInfo, PropertyInfo, RelationCacheKey,
    RelationFlags, TypeId, TypeParamInfo,
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
fn assignability_cache_strict_any_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);

    let ordinary = RelationPolicy::default().with_strict_any_propagation(false);
    let strict_any = RelationPolicy::default().with_strict_any_propagation(true);
    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let strict_any_key =
        RelationCacheKey::for_assignability(source, target, strict_any.cache_config());

    assert_ne!(
        ordinary_key, strict_any_key,
        "ordinary and strict-any policies must occupy distinct assignability cache slots",
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
    let strict_any_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_any,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary assignability should allow nested `any` to satisfy a number property",
    );
    assert!(
        !strict_any_uncached,
        "strict-any assignability must not let nested `any` silence the property mismatch",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary any policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary any result must be stored in the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_any_key),
        None,
        "strict-any lookup must not hit the ordinary any slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict_any),
        strict_any_uncached,
        "cached strict-any policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_any_key),
        Some(strict_any_uncached),
        "strict-any result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary any slot must remain intact after the strict-any lookup",
    );
}

#[test]
fn assignability_cache_strict_function_types_does_not_imply_strict_any_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);

    let strict_functions =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let strict_any = strict_functions.with_strict_any_propagation(true);
    let strict_functions_key =
        RelationCacheKey::for_assignability(source, target, strict_functions.cache_config());
    let strict_any_key =
        RelationCacheKey::for_assignability(source, target, strict_any.cache_config());

    assert_ne!(
        strict_functions_key, strict_any_key,
        "strict-function and strict-any policies must occupy distinct assignability cache slots",
    );

    let strict_functions_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_functions,
        RelationContext::default(),
    )
    .is_related();
    let strict_any_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_any,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        strict_functions_uncached,
        "strict function types alone should still allow nested `any` to satisfy a number property",
    );
    assert!(
        !strict_any_uncached,
        "explicit strict-any propagation must reject the nested `any` property mismatch",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict_functions),
        strict_functions_uncached,
        "cached strict-function policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_functions_key),
        Some(strict_functions_uncached),
        "strict-function result must be stored in the strict-function assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_any_key),
        None,
        "strict-any lookup must not hit the strict-function slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict_any),
        strict_any_uncached,
        "cached strict-any policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_any_key),
        Some(strict_any_uncached),
        "strict-any result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_functions_key),
        Some(strict_functions_uncached),
        "strict-function slot must remain intact after the strict-any lookup",
    );
}

#[test]
fn assignability_cache_skip_weak_type_checks_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let unrelated = interner.intern_string("unrelated");
    let optional = interner.intern_string("optional");

    let source = interner.object(vec![PropertyInfo::new(unrelated, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::opt(optional, TypeId::NUMBER)]);

    let ordinary = RelationPolicy::default().with_skip_weak_type_checks(false);
    let skip_weak = RelationPolicy::default().with_skip_weak_type_checks(true);
    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let skip_weak_key =
        RelationCacheKey::for_assignability(source, target, skip_weak.cache_config());

    assert_ne!(
        ordinary_key, skip_weak_key,
        "ordinary and skip-weak policies must occupy distinct assignability cache slots",
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
    let skip_weak_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        skip_weak,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !ordinary_uncached,
        "ordinary assignability should reject unrelated object properties against a weak target",
    );
    assert!(
        skip_weak_uncached,
        "skip-weak assignability should bypass the weak-type no-overlap rejection",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary weak-type policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary weak-type result must be stored in the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(skip_weak_key),
        None,
        "skip-weak lookup must not hit the ordinary weak-type slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, skip_weak),
        skip_weak_uncached,
        "cached skip-weak policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(skip_weak_key),
        Some(skip_weak_uncached),
        "skip-weak result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary weak-type slot must remain intact after the skip-weak lookup",
    );
}

#[test]
fn assignability_cache_strict_null_checks_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let source = TypeId::UNDEFINED;
    let target = TypeId::NUMBER;

    let loose = RelationPolicy::unflagged_compatibility();
    let strict_null = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let loose_key = RelationCacheKey::for_assignability(source, target, loose.cache_config());
    let strict_null_key =
        RelationCacheKey::for_assignability(source, target, strict_null.cache_config());

    assert_ne!(
        loose_key, strict_null_key,
        "loose and strict-null policies must occupy distinct assignability cache slots",
    );

    let loose_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        loose,
        RelationContext::default(),
    )
    .is_related();
    let strict_null_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_null,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        loose_uncached,
        "loose nullability should allow `undefined` to satisfy a number target",
    );
    assert!(
        !strict_null_uncached,
        "strict null checks should reject `undefined` for a number target",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, loose),
        loose_uncached,
        "cached loose nullability policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(loose_key),
        Some(loose_uncached),
        "loose nullability result must be stored in the loose assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_null_key),
        None,
        "strict-null lookup must not hit the loose nullability slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict_null),
        strict_null_uncached,
        "cached strict-null policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_null_key),
        Some(strict_null_uncached),
        "strict-null result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(loose_key),
        Some(loose_uncached),
        "loose nullability slot must remain intact after the strict-null lookup",
    );
}

#[test]
fn assignability_cache_assume_related_on_cycle_matches_uncached_depth_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let mut source = TypeId::STRING;
    let mut target = TypeId::NUMBER;
    for _ in 0..128 {
        source = interner.array(source);
        target = interner.array(target);
    }

    let assume_related = RelationPolicy::default().with_assume_related_on_cycle(true);
    let reject_overflow = RelationPolicy::default().with_assume_related_on_cycle(false);
    let assume_key =
        RelationCacheKey::for_assignability(source, target, assume_related.cache_config());
    let reject_key =
        RelationCacheKey::for_assignability(source, target, reject_overflow.cache_config());

    assert_ne!(
        assume_key, reject_key,
        "cycle/depth-overflow policies must occupy distinct assignability cache slots",
    );

    let assume_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        assume_related,
        RelationContext::default(),
    );
    let reject_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        reject_overflow,
        RelationContext::default(),
    );

    assert!(
        assume_uncached.depth_exceeded,
        "deep nested array comparison should exceed the relation depth budget",
    );
    assert!(
        reject_uncached.depth_exceeded,
        "the non-assuming policy should see the same depth overflow",
    );
    assert!(
        assume_uncached.is_related(),
        "assume-related policy should treat relation depth overflow as related",
    );
    assert!(
        !reject_uncached.is_related(),
        "non-assuming policy should treat relation depth overflow as not related",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, assume_related),
        assume_uncached.is_related(),
        "cached assume-related overflow policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(assume_key),
        Some(assume_uncached.is_related()),
        "assume-related result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(reject_key),
        None,
        "non-assuming lookup must not hit the assume-related slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, reject_overflow),
        reject_uncached.is_related(),
        "cached non-assuming overflow policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(reject_key),
        Some(reject_uncached.is_related()),
        "non-assuming result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(assume_key),
        Some(assume_uncached.is_related()),
        "assume-related slot must remain intact after the non-assuming lookup",
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

    let strict_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let loose_policy = RelationPolicy::unflagged_compatibility();

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
fn assignability_cache_callable_signatures_match_uncached_function_variance() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::unnamed(dog)],
            TypeId::VOID,
        )],
        ..CallableShape::default()
    });
    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(
            vec![ParamInfo::unnamed(animal)],
            TypeId::VOID,
        )],
        ..CallableShape::default()
    });

    let strict_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let loose_policy = RelationPolicy::unflagged_compatibility();
    let strict_key =
        RelationCacheKey::for_assignability(source, target, strict_policy.cache_config());
    let loose_key =
        RelationCacheKey::for_assignability(source, target, loose_policy.cache_config());

    assert_ne!(
        strict_key, loose_key,
        "callable strict and loose variance policies must occupy distinct assignability cache slots",
    );

    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_policy,
        RelationContext::default(),
    )
    .is_related();
    let loose_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        loose_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !strict_uncached,
        "strict callable parameter variance should reject `(dog) => void` where `(animal) => void` is required",
    );
    assert!(
        loose_uncached,
        "loose callable parameter variance should accept the bivariant call-signature comparison",
    );

    let strict_cached = db.is_assignable_to_with_policy(source, target, strict_policy);
    assert_eq!(
        strict_cached, strict_uncached,
        "cached strict callable variance must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_cached),
        "strict callable result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(loose_key),
        None,
        "loose callable lookup must not hit the strict callable slot",
    );

    let loose_cached = db.is_assignable_to_with_policy(source, target, loose_policy);
    assert_eq!(
        loose_cached, loose_uncached,
        "cached loose callable variance must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(loose_key),
        Some(loose_cached),
        "loose callable result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_cached),
        "strict callable slot must remain intact after the loose callable lookup",
    );
}

#[test]
fn subtype_cache_allow_void_return_matches_uncached_function_return_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(vec![], TypeId::STRING));
    let target = interner.function(FunctionShape::new(vec![], TypeId::VOID));

    let strict_policy = RelationPolicy::unflagged_compatibility();
    let allow_void_policy = RelationPolicy::from_relation_flags(RelationFlags::ALLOW_VOID_RETURN);

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

    let inexact_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let exact_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES,
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
fn assignability_cache_no_unchecked_indexed_access_matches_uncached_index_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let source = interner.index_access(interner.array(TypeId::STRING), TypeId::NUMBER);
    let target = TypeId::STRING;
    let checked_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let unchecked_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::NO_UNCHECKED_INDEXED_ACCESS,
    );
    let checked_key =
        RelationCacheKey::for_assignability(source, target, checked_policy.cache_config());
    let unchecked_key =
        RelationCacheKey::for_assignability(source, target, unchecked_policy.cache_config());

    assert_ne!(
        checked_key, unchecked_key,
        "no-unchecked-indexed-access must occupy a distinct assignability cache slot",
    );

    let checked_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        checked_policy,
        RelationContext::default(),
    )
    .is_related();
    let unchecked_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        unchecked_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        checked_uncached,
        "plain array indexed access should produce the element type",
    );
    assert!(
        !unchecked_uncached,
        "no-unchecked-indexed-access should include undefined under strict null checks",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, checked_policy),
        checked_uncached,
        "cached checked indexed-access policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(checked_key),
        Some(checked_uncached),
        "checked indexed-access result must be stored in the checked assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(unchecked_key),
        None,
        "unchecked lookup must not hit the checked slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, unchecked_policy),
        unchecked_uncached,
        "cached no-unchecked-indexed-access policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(unchecked_key),
        Some(unchecked_uncached),
        "unchecked indexed-access result must be stored in the unchecked assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(checked_key),
        Some(checked_uncached),
        "checked assignability slot must remain intact after the unchecked lookup",
    );
}

#[test]
fn subtype_cache_strict_readonly_identity_matches_uncached_property_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let x = interner.intern_string("x");

    let source = interner.object(vec![PropertyInfo::readonly(x, TypeId::NUMBER)]);
    let target = interner.object(vec![PropertyInfo::new(x, TypeId::NUMBER)]);

    let permissive_policy = RelationPolicy::unflagged_compatibility();
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

    let ordinary = RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES)
        .with_strict_subtype_checking(false);
    let strict = RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES)
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

#[test]
fn assignability_cache_allow_bivariant_rest_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    ));
    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        TypeId::VOID,
    ));

    let ordinary = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::STRICT_NULL_CHECKS,
    )
    .with_strict_any_propagation(true)
    .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);
    let bivariant_rest = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::STRICT_NULL_CHECKS
            | RelationFlags::ALLOW_BIVARIANT_REST,
    )
    .with_strict_any_propagation(true)
    .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);
    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let bivariant_rest_key =
        RelationCacheKey::for_assignability(source, target, bivariant_rest.cache_config());

    assert_ne!(
        ordinary_key, bivariant_rest_key,
        "ordinary and bivariant-rest policies must occupy distinct assignability cache slots",
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
    let bivariant_rest_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_rest,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !ordinary_uncached,
        "ordinary strict-any assignability should compare extra parameters normally",
    );
    assert!(
        bivariant_rest_uncached,
        "bivariant-rest assignability should accept fixed arguments against a rest-`any` target",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary rest policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary rest result must be stored in the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_rest_key),
        None,
        "bivariant-rest lookup must not hit the ordinary slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, bivariant_rest),
        bivariant_rest_uncached,
        "cached bivariant-rest policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_rest_key),
        Some(bivariant_rest_uncached),
        "bivariant-rest result must be stored in the bivariant-rest assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary rest slot must remain intact after the bivariant-rest lookup",
    );
}

#[test]
fn assignability_cache_allow_bivariant_param_count_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    ));
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::STRING)],
        TypeId::VOID,
    ));

    let ordinary = RelationPolicy::unflagged_compatibility();
    let bivariant_count =
        RelationPolicy::from_relation_flags(RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT);
    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let bivariant_count_key =
        RelationCacheKey::for_assignability(source, target, bivariant_count.cache_config());

    assert_ne!(
        ordinary_key, bivariant_count_key,
        "ordinary and bivariant-param-count policies must occupy distinct assignability cache slots",
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
    let bivariant_count_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_count,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !ordinary_uncached,
        "ordinary assignability should reject extra required source parameters",
    );
    assert!(
        bivariant_count_uncached,
        "bivariant parameter-count assignability should allow extra required source parameters",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary parameter-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary parameter-count result must be stored in the ordinary assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_count_key),
        None,
        "bivariant-param-count lookup must not hit the ordinary slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, bivariant_count),
        bivariant_count_uncached,
        "cached bivariant-param-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_count_key),
        Some(bivariant_count_uncached),
        "bivariant-param-count result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary parameter-count slot must remain intact after the bivariant-param-count lookup",
    );
}

#[test]
fn assignability_cache_erase_generics_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let target_t = TypeParamInfo {
        name: interner.intern_string("Target"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_t_type = interner.type_param(target_t);
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: target_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target = interner.function(FunctionShape {
        type_params: vec![target_t],
        params: vec![],
        this_type: None,
        return_type: target_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let erased = RelationPolicy::default().with_erase_generics(true);
    let strict = RelationPolicy::default().with_erase_generics(false);
    let erased_key = RelationCacheKey::for_assignability(source, target, erased.cache_config());
    let strict_key = RelationCacheKey::for_assignability(source, target, strict.cache_config());

    assert_ne!(
        erased_key, strict_key,
        "erased and strict generic-signature policies must occupy distinct assignability cache slots",
    );

    let erased_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        erased,
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
        erased_uncached,
        "erased generic-signature compatibility should allow the relation",
    );
    assert!(
        !strict_uncached,
        "strict generic-signature compatibility must not promote an outer type parameter into a generic signature",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict),
        strict_uncached,
        "cached strict generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_uncached),
        "strict generic result must be stored in the strict assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        None,
        "erased-generic lookup must not hit the strict slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, erased),
        erased_uncached,
        "cached erased generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        Some(erased_uncached),
        "erased generic result must be stored in the erased assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_uncached),
        "strict generic slot must remain intact after the erased lookup",
    );
}

#[test]
fn assignability_cache_erased_generic_retry_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let source_s = TypeParamInfo {
        name: interner.intern_string("Source"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let source_s_type = interner.type_param(source_s);
    let source = interner.function(FunctionShape {
        type_params: vec![source_s],
        params: vec![ParamInfo::unnamed(source_s_type)],
        this_type: None,
        return_type: source_s_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target_t = TypeParamInfo {
        name: interner.intern_string("TargetT"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_u = TypeParamInfo {
        name: interner.intern_string("TargetU"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let target_t_type = interner.type_param(target_t);
    let target_u_type = interner.type_param(target_u);
    let target = interner.function(FunctionShape {
        type_params: vec![target_t, target_u],
        params: vec![ParamInfo::unnamed(target_t_type)],
        this_type: None,
        return_type: target_u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let no_retry = RelationPolicy::default();
    let retry =
        RelationPolicy::from_relation_flags(RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY);
    let no_retry_key = RelationCacheKey::for_assignability(source, target, no_retry.cache_config());
    let retry_key = RelationCacheKey::for_assignability(source, target, retry.cache_config());

    assert_ne!(
        no_retry_key, retry_key,
        "erased generic retry policy must occupy a distinct assignability cache slot",
    );

    let no_retry_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        no_retry,
        RelationContext::default(),
    )
    .is_related();
    let retry_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        retry,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !no_retry_uncached,
        "contextual inference should reject the unequal-arity generic signatures before retry",
    );
    assert!(
        retry_uncached,
        "erased generic retry should allow the unequal-arity signatures",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, no_retry),
        no_retry_uncached,
        "cached no-retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(no_retry_uncached),
        "no-retry result must be stored in the no-retry assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        None,
        "retry lookup must not hit the no-retry slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, retry),
        retry_uncached,
        "cached retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        Some(retry_uncached),
        "retry result must be stored in its own assignability slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(no_retry_uncached),
        "no-retry slot must remain intact after the retry lookup",
    );
}

#[test]
fn assignability_cache_in_callback_param_check_matches_uncached_relation_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let name = interner.intern_string("name");
    let breed = interner.intern_string("breed");

    let animal = interner.object(vec![PropertyInfo::new(name, TypeId::STRING)]);
    let dog = interner.object(vec![
        PropertyInfo::new(name, TypeId::STRING),
        PropertyInfo::new(breed, TypeId::STRING),
    ]);

    let mut dog_method_shape = FunctionShape::new(vec![ParamInfo::unnamed(dog)], TypeId::VOID);
    dog_method_shape.is_method = true;
    let source = interner.function(dog_method_shape);

    let mut animal_method_shape =
        FunctionShape::new(vec![ParamInfo::unnamed(animal)], TypeId::VOID);
    animal_method_shape.is_method = true;
    let target = interner.function(animal_method_shape);

    let ordinary_method_policy =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let callback_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::IN_CALLBACK_PARAM_CHECK,
    );
    let ordinary_key =
        RelationCacheKey::for_assignability(source, target, ordinary_method_policy.cache_config());
    let callback_key =
        RelationCacheKey::for_assignability(source, target, callback_policy.cache_config());

    assert_ne!(
        ordinary_key, callback_key,
        "callback parameter mode must occupy a distinct assignability cache slot",
    );

    let ordinary_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        ordinary_method_policy,
        RelationContext::default(),
    )
    .is_related();
    let callback_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        callback_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary strict-function method comparison keeps method parameters bivariant",
    );
    assert!(
        !callback_uncached,
        "callback parameter mode must disable method bivariance for the immediate signature comparison",
    );

    let ordinary_cached = db.is_assignable_to_with_policy(source, target, ordinary_method_policy);
    assert_eq!(
        ordinary_cached, ordinary_uncached,
        "cached ordinary method policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_cached),
        "ordinary method result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(callback_key),
        None,
        "callback-mode lookup must not hit the ordinary method slot",
    );

    let callback_cached = db.is_assignable_to_with_policy(source, target, callback_policy);
    assert_eq!(
        callback_cached, callback_uncached,
        "cached callback-mode policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(callback_key),
        Some(callback_cached),
        "callback-mode result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(ordinary_key),
        Some(ordinary_cached),
        "ordinary method slot must remain intact after the callback-mode lookup",
    );
}

#[test]
fn assignability_cache_disable_method_bivariance_matches_uncached_method_parameter_count() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let mut source_shape = FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    );
    source_shape.is_method = true;
    let source = interner.function(source_shape);

    let mut target_shape =
        FunctionShape::new(vec![ParamInfo::unnamed(TypeId::STRING)], TypeId::VOID);
    target_shape.is_method = true;
    let target = interner.function(target_shape);

    let bivariant_method = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT,
    );
    let sound_method = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
            | RelationFlags::DISABLE_METHOD_BIVARIANCE,
    );
    let bivariant_key =
        RelationCacheKey::for_assignability(source, target, bivariant_method.cache_config());
    let sound_key =
        RelationCacheKey::for_assignability(source, target, sound_method.cache_config());

    assert_ne!(
        bivariant_key, sound_key,
        "method-bivariant and sound-method parameter-count policies must occupy distinct assignability cache slots",
    );

    let bivariant_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_method,
        RelationContext::default(),
    )
    .is_related();
    let sound_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        sound_method,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        bivariant_uncached,
        "method bivariance should allow extra required method parameters when the count exception is enabled",
    );
    assert!(
        !sound_uncached,
        "disabling method bivariance should also disable the method parameter-count exception",
    );

    let bivariant_cached = db.is_assignable_to_with_policy(source, target, bivariant_method);
    assert_eq!(
        bivariant_cached, bivariant_uncached,
        "cached method-bivariant parameter-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_key),
        Some(bivariant_cached),
        "method-bivariant parameter-count result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(sound_key),
        None,
        "sound-method lookup must not hit the method-bivariant parameter-count slot",
    );

    let sound_cached = db.is_assignable_to_with_policy(source, target, sound_method);
    assert_eq!(
        sound_cached, sound_uncached,
        "cached sound-method parameter-count policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(sound_key),
        Some(sound_cached),
        "sound-method parameter-count result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(bivariant_key),
        Some(bivariant_cached),
        "method-bivariant parameter-count slot must remain intact after the sound-method lookup",
    );
}
