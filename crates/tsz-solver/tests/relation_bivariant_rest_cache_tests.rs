//! Cache agreement tests for bivariant rest-parameter relation policy.

use super::*;
use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::types::{FunctionShape, ParamInfo, RelationCacheKey, RelationFlags};

fn void_function(params: Vec<ParamInfo>) -> FunctionShape {
    FunctionShape {
        params,
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    }
}

#[test]
fn subtype_cache_bivariant_rest_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let rest = interner.intern_string("args");
    let source = interner.function(void_function(vec![ParamInfo::unnamed(TypeId::STRING)]));
    let target = interner.function(void_function(vec![ParamInfo::rest(
        rest,
        interner.array(TypeId::UNKNOWN),
    )]));

    let strict = RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let bivariant = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::ALLOW_BIVARIANT_REST,
    );
    let strict_key = RelationCacheKey::for_subtype(source, target, strict.cache_config());
    let bivariant_key = RelationCacheKey::for_subtype(source, target, bivariant.cache_config());

    assert_ne!(
        strict_key, bivariant_key,
        "strict and bivariant rest policies must occupy distinct cache slots",
    );

    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        strict,
        RelationContext::default(),
    )
    .is_related();
    let bivariant_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        bivariant,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !strict_uncached,
        "strict rest policy should reject fixed string param against unknown rest target",
    );
    assert!(
        bivariant_uncached,
        "bivariant rest policy should treat unknown rest target as top-like",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(source, target, strict),
        strict_uncached,
        "cached strict rest policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        Some(strict_uncached),
        "strict rest result must be stored in the strict slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(bivariant_key),
        None,
        "bivariant rest lookup must not hit the strict slot",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(source, target, bivariant),
        bivariant_uncached,
        "cached bivariant rest policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(bivariant_key),
        Some(bivariant_uncached),
        "bivariant rest result must be stored in the bivariant slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        Some(strict_uncached),
        "strict rest slot must remain intact after the bivariant lookup",
    );
}
