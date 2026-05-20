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
fn application_eval_cache_is_per_file_isolated() {
    // Structural rule: `application_eval_cache` is intentionally NOT shared
    // cross-file. Parallel file checking can observe incomplete lib-merge state
    // on the first evaluation of a generic type alias (e.g. `Promise<T>`),
    // producing a stale result that would poison sibling files if shared.
    // Each file checker gets an independent local cache; results are never
    // promoted to or read from the `SharedQueryCache`.
    // See issue #9507.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();

    let def_id = DefId(1);
    let args = &[TypeId::STRING];
    let result = TypeId::NUMBER;

    // File A evaluates `Alias<string>` and populates its local cache.
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

    // File B gets a fresh per-file cache. Its local cache starts empty;
    // the shared cache does NOT hold application_eval entries, so B must
    // recompute the result independently.
    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_application_eval_cache(def_id, args, false),
            None,
            "application_eval_cache is per-file; file B must not see file A's result"
        );
        let stats = db_b.statistics();
        assert_eq!(stats.application_eval_cache_hits, 0);
        assert_eq!(stats.application_eval_cache_misses, 1);
    }

    // Shared cache itself holds no application_eval entries.
    assert_eq!(
        shared.total_entries(),
        0,
        "SharedQueryCache must not store application_eval_cache entries"
    );
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
fn instantiation_cache_is_per_file_isolated() {
    // Structural rule: `instantiation_cache` is intentionally NOT shared
    // cross-file. The same class of ordering-sensitivity that affects
    // `application_eval_cache` (incomplete lib-merge state on first evaluation)
    // also applies to generic body instantiation. Sharing would cause stale
    // instantiated TypeIds to leak across file boundaries.
    // See issue #9507.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();
    let key = InstantiationCacheKey::new(TypeId::STRING, CanonicalSubst::empty(), 0, None);
    let result = TypeId::NUMBER;

    // File A instantiates the body and populates its local cache only.
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

    // File B has a fresh local cache; the shared cache holds no instantiation
    // entries, so B sees a miss and must recompute independently.
    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_instantiation_cache(&key),
            None,
            "instantiation_cache is per-file; file B must not see file A's result"
        );

        let stats = db_b.statistics();
        assert_eq!(stats.instantiation_cache_entries, 0);
        assert_eq!(stats.instantiation_cache_hits, 0);
        assert_eq!(stats.instantiation_cache_misses, 1);
    }

    // Shared cache itself holds no instantiation entries.
    assert_eq!(
        shared.total_entries(),
        0,
        "SharedQueryCache must not store instantiation_cache entries"
    );
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
