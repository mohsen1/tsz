//! Query cache statistics and size-accounting coverage tests.

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::{QueryCache, QueryCacheStatistics};
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
