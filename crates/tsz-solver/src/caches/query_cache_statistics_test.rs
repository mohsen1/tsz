//! Query cache statistics and size-accounting coverage tests.

use crate::caches::db::QueryDatabase;
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::caches::query_cache::{QueryCache, QueryCacheStatistics, SharedQueryCache};
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::types::TypeId;

#[test]
fn intersection_merge_cache_is_visible_in_statistics_and_size_estimate() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();

    assert_eq!(before.intersection_merge_cache_entries, 0);
    assert_eq!(before.intersection_merge_cache_hits, 0);
    assert_eq!(before.intersection_merge_cache_misses, 0);

    assert_eq!(db.lookup_intersection_merge(TypeId::STRING), None);
    db.insert_intersection_merge(TypeId::STRING, Some(TypeId::NUMBER));
    assert_eq!(
        db.lookup_intersection_merge(TypeId::STRING),
        Some(Some(TypeId::NUMBER))
    );

    let after = db.statistics();
    assert_eq!(after.intersection_merge_cache_entries, 1);
    assert_eq!(after.intersection_merge_cache_hits, 1);
    assert_eq!(after.intersection_merge_cache_misses, 1);
    assert!(after.estimated_size_bytes() > before.estimated_size_bytes());
    assert!(db.estimated_size_bytes() > before.estimated_size_bytes());

    let rendered = after.to_string();
    assert!(rendered.contains("intersection_merge"));
    assert!(rendered.contains("1 hits, 1 misses"));
}

#[test]
fn application_eval_cache_is_file_local() {
    // Structural rule: `application_eval_cache` results are NOT shared
    // cross-file. The evaluation of `(DefId, [TypeId])` depends on the
    // per-file resolver's body registration for that `DefId`; the same key can
    // produce different results in two files (e.g., async/Promise families where
    // the alias body is registered at different points in parallel checking).
    // Sharing stale results across files causes conformance regressions.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();

    let def_id = DefId(1);
    let args = &[TypeId::STRING];
    let result = TypeId::NUMBER;

    // File A evaluates `Alias<string>` and stores the result locally.
    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_a.lookup_application_eval_cache(def_id, args, false),
            None
        );
        db_a.insert_application_eval_cache(def_id, args, false, result);
        assert_eq!(
            db_a.lookup_application_eval_cache(def_id, args, false),
            Some(result)
        );
        let stats = db_a.statistics();
        assert_eq!(stats.application_eval_cache_entries, 1);
        assert_eq!(stats.application_eval_cache_hits, 1);
        assert_eq!(stats.application_eval_cache_misses, 1);
    }

    // File B gets a fresh per-file cache. It must NOT inherit file A's result:
    // the resolver may return a different body for the same DefId in file B's
    // context (e.g., the body is not yet registered, or a different overload is
    // visible). File B must recompute from scratch.
    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_application_eval_cache(def_id, args, false),
            None,
            "application_eval_cache must not be shared cross-file"
        );
        let stats = db_b.statistics();
        assert_eq!(stats.application_eval_cache_hits, 0, "no cross-file hits");
        assert_eq!(
            stats.application_eval_cache_misses, 1,
            "file-B lookup should be a miss"
        );
    }

    // A flag difference (`noUncheckedIndexedAccess=true`) must not hit the
    // false-keyed entry even within the same file.
    {
        let db_c = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_c.lookup_application_eval_cache(def_id, args, true),
            None,
            "flag-distinct key must not alias an entry stored with a different flag"
        );
    }
}

#[test]
fn application_eval_cache_stats_visible_in_display() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    db.insert_application_eval_cache(DefId(1), &[TypeId::STRING], false, TypeId::NUMBER);
    let _ = db.lookup_application_eval_cache(DefId(1), &[TypeId::STRING], false);
    let stats = db.statistics();
    let rendered = stats.to_string();
    assert!(
        rendered.contains("application_eval_cache"),
        "statistics display should mention application_eval_cache"
    );
    assert!(
        rendered.contains("hits"),
        "statistics display should report hits"
    );
}

#[test]
fn instantiation_cache_is_file_local() {
    // Structural rule: `instantiation_cache` results are NOT shared cross-file.
    // Instantiation of a generic body can depend on `Lazy(DefId)` resolution
    // that is per-file; a result produced in one file's context would be stale
    // in another file where different `DefId` bindings are registered.
    // Async/Promise/dynamic-import families are the primary witnesses.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();
    let key = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);
    let result = TypeId::NUMBER;

    // File A instantiates the body and stores it in its local cache only.
    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(db_a.lookup_instantiation_cache(&key), None);
        db_a.insert_instantiation_cache(key.clone(), result);
        assert_eq!(db_a.lookup_instantiation_cache(&key), Some(result));

        let stats = db_a.statistics();
        assert_eq!(stats.instantiation_cache_entries, 1);
        assert_eq!(stats.instantiation_cache_hits, 1);
        assert_eq!(stats.instantiation_cache_misses, 1);
    }

    // File B has a fresh local cache and must NOT inherit file A's result.
    // The same instantiation key may resolve differently in file B's context.
    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_instantiation_cache(&key),
            None,
            "instantiation_cache must not be shared cross-file"
        );

        let stats = db_b.statistics();
        assert_eq!(stats.instantiation_cache_entries, 0);
        assert_eq!(stats.instantiation_cache_hits, 0);
        assert_eq!(
            stats.instantiation_cache_misses, 1,
            "file-B lookup should be a miss"
        );
    }

    // Mode-bit and `this` differences are semantically distinct instantiation
    // requests and must not alias the entry stored above (within the same file).
    {
        let db_c = QueryCache::new_with_shared(&interner, &shared);
        let mode_distinct_key =
            InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 1, None);
        let this_distinct_key = InstantiationCacheKey::new(
            TypeId::STRING,
            CanonicalSubst::empty(),
            0,
            Some(TypeId::STRING),
        );

        assert_eq!(
            db_c.lookup_instantiation_cache(&mode_distinct_key),
            None,
            "mode-bit-distinct key must not alias a cached instantiation"
        );
        assert_eq!(
            db_c.lookup_instantiation_cache(&this_distinct_key),
            None,
            "`this`-distinct key must not alias a cached instantiation"
        );
    }
}

#[test]
fn query_cache_statistics_merge_includes_intersection_merge_cache() {
    let mut left = QueryCacheStatistics {
        intersection_merge_cache_entries: 2,
        intersection_merge_cache_hits: 3,
        intersection_merge_cache_misses: 5,
        ..Default::default()
    };
    let right = QueryCacheStatistics {
        intersection_merge_cache_entries: 7,
        intersection_merge_cache_hits: 11,
        intersection_merge_cache_misses: 13,
        ..Default::default()
    };

    left.merge(&right);

    assert_eq!(left.intersection_merge_cache_entries, 9);
    assert_eq!(left.intersection_merge_cache_hits, 14);
    assert_eq!(left.intersection_merge_cache_misses, 18);
}
