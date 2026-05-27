//! Cache agreement tests for `NO_UNCHECKED_INDEXED_ACCESS` relation policy.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation,
};
use crate::types::{RelationCacheKey, TypeData, TypeId};

#[test]
fn subtype_cache_no_unchecked_indexed_access_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let indexed_string = interner.intern(TypeData::IndexAccess(string_array, TypeId::NUMBER));

    let ordinary = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS);
    let no_unchecked = RelationPolicy::from_flags(
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS
            | RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS,
    );
    let ordinary_key =
        RelationCacheKey::for_subtype(indexed_string, TypeId::STRING, ordinary.cache_config());
    let no_unchecked_key =
        RelationCacheKey::for_subtype(indexed_string, TypeId::STRING, no_unchecked.cache_config());

    assert_ne!(
        ordinary_key, no_unchecked_key,
        "ordinary and no-unchecked-indexed-access policies must occupy distinct subtype cache slots",
    );

    let uncached_ordinary = query_relation(
        &interner,
        indexed_string,
        TypeId::STRING,
        RelationKind::Subtype,
        ordinary,
        RelationContext::default(),
    )
    .is_related();
    let uncached_no_unchecked = query_relation(
        &interner,
        indexed_string,
        TypeId::STRING,
        RelationKind::Subtype,
        no_unchecked,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        uncached_ordinary,
        "ordinary indexed access should resolve `string[][number]` to `string`",
    );
    assert!(
        !uncached_no_unchecked,
        "no-unchecked-indexed-access should add `undefined` and reject subtype-of `string`",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(indexed_string, TypeId::STRING, ordinary),
        uncached_ordinary,
        "cached ordinary policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(ordinary_key),
        Some(uncached_ordinary),
        "ordinary result must be stored in the ordinary subtype slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(no_unchecked_key),
        None,
        "no-unchecked lookup must not hit the ordinary subtype slot",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(indexed_string, TypeId::STRING, no_unchecked),
        uncached_no_unchecked,
        "cached no-unchecked policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(no_unchecked_key),
        Some(uncached_no_unchecked),
        "no-unchecked result must be stored in the no-unchecked subtype slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(ordinary_key),
        Some(uncached_ordinary),
        "ordinary subtype slot must remain intact after the no-unchecked lookup",
    );
}
