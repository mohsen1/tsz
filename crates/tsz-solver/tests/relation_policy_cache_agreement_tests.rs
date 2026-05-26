//! Cache-enabled/cache-disabled agreement tests for behavior-changing relation
//! policies.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{PropertyInfo, TypeId};

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
