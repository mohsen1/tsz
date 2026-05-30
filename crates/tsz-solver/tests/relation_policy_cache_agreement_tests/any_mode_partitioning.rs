//! Effective `any`-propagation cache partitioning tests.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::{AnyPropagationMode, SubtypeChecker};
use crate::types::{CachedAnyMode, PropertyInfo, RelationCacheKey, TypeId};

#[test]
fn subtype_cache_top_level_only_any_mode_partitions_nested_lookup() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");

    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);
    let policy =
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);
    let mut cached_checker = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);

    let top_level_key = cached_checker.debug_cache_key_for(source, target);
    let nested_config = top_level_key
        .config
        .with_any_mode(CachedAnyMode::TopLevelOnlyNested);
    let nested_property_key =
        RelationCacheKey::for_subtype(TypeId::STRICT_ANY, TypeId::NUMBER, nested_config);
    let wrong_top_level_property_key =
        RelationCacheKey::for_subtype(TypeId::STRICT_ANY, TypeId::NUMBER, top_level_key.config);

    assert_ne!(
        nested_property_key, wrong_top_level_property_key,
        "nested top-level-only any checks must not alias the top-level any-mode slot",
    );

    let uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let cached = cached_checker.check_subtype(source, target).is_true();

    assert!(
        !uncached,
        "top-level-only any propagation should reject a nested `any` property mismatch",
    );
    assert_eq!(
        cached, uncached,
        "cached top-level-only subtype must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_subtype_cache(top_level_key),
        Some(cached),
        "outer object relation must use the top-level any-mode cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(nested_property_key),
        Some(false),
        "nested property relation must use the nested any-mode cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(wrong_top_level_property_key),
        None,
        "nested property relation must not populate the top-level any-mode slot",
    );
}
