//! Query cache statistics and size-accounting coverage tests.

use crate::caches::db::{QueryDatabase, TypeApplicationEvalCache};
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::caches::query_cache::{QueryCache, SharedQueryCache};
use crate::caches::query_cache_statistics::QueryCacheStatistics;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::types::{RelationCacheConfig, RelationCacheKey, TypeId};

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
fn closed_eval_cache_is_visible_in_statistics_and_size_estimate() {
    // Structural rule: `closed_eval_cache` is a per-`QueryCache`
    // substitution-independent evaluation cache. It is cleared with the rest
    // of the query cache and is safe to observe through aggregate residency
    // stats without changing its key, eligibility gate, or sharing behavior.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();

    assert_eq!(before.closed_eval_cache_entries, 0);

    assert_eq!(db.lookup_closed_eval_cache(TypeId::STRING, false), None);
    db.insert_closed_eval_cache(TypeId::STRING, false, TypeId::NUMBER);
    assert_eq!(
        db.lookup_closed_eval_cache(TypeId::STRING, false),
        Some(TypeId::NUMBER)
    );

    let after = db.statistics();
    assert_eq!(after.closed_eval_cache_entries, 1);
    assert!(after.estimated_size_bytes() > before.estimated_size_bytes());
    assert!(db.estimated_size_bytes() > before.estimated_size_bytes());

    let rendered = after.to_string();
    assert!(rendered.contains("closed_eval_cache"));

    db.clear();
    assert_eq!(db.statistics().closed_eval_cache_entries, 0);
}

#[test]
fn conditional_branch_verdict_cache_round_trips_and_is_key_partitioned() {
    // Structural rule (issues #8356 / #13097): the conditional-branch verdict
    // cache is a per-`QueryCache` map keyed by
    // `(check, extends, no_unchecked_indexed_access)`. It stores `bool`
    // verdicts (a distinct relation from plain subtyping), is partitioned by
    // the `no_unchecked_indexed_access` flag, and is cleared with the rest of
    // the query cache. Raw-interner backends opt out (default `None`/no-op).
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();
    assert_eq!(before.conditional_branch_verdict_cache_entries, 0);

    // Miss on an empty cache.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false),
        None
    );

    // Round-trip a `true` verdict.
    db.insert_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false),
        Some(true)
    );

    // The entry is visible in statistics / size accounting (residency tooling).
    let after = db.statistics();
    assert_eq!(after.conditional_branch_verdict_cache_entries, 1);
    assert!(after.estimated_size_bytes() > before.estimated_size_bytes());
    assert!(after.to_string().contains("cond_branch_verdict"));

    // The `no_unchecked_indexed_access` flag partitions the key: the same
    // type pair under the other flag value is a distinct, still-empty slot.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, true),
        None
    );
    // Operand order matters — `check`/`extends` are not symmetric.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false),
        None
    );

    // A `false` verdict round-trips distinctly from an absent entry.
    db.insert_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false),
        Some(false)
    );

    // Cleared with the rest of the query cache.
    db.clear();
    assert_eq!(db.statistics().conditional_branch_verdict_cache_entries, 0);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false),
        None
    );
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false),
        None
    );
}

#[test]
fn conditional_branch_verdict_cache_defaults_off_for_raw_interner() {
    // The trait default is a no-op so raw `TypeInterner` backends and tests
    // opt out: a lookup always misses and an insert is dropped.
    let interner = TypeInterner::new();
    interner.insert_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true);
    assert_eq!(
        interner.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false),
        None
    );
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
fn opt_in_shared_application_eval_cache_reuses_across_file_caches() {
    // Structural rule: #13240's shared application-eval cache stays opt-in,
    // but once a shared cache is explicitly opted in, sibling file-local
    // `QueryCache`s can reuse the same generic application answer and expose
    // that reuse through shared hit/miss/insert counters.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new_for_instantiation_family_test(true);

    let def_id = DefId(7);
    let args = &[TypeId::STRING, TypeId::NUMBER];
    let result = TypeId::BOOLEAN;

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_a.lookup_application_eval_cache(def_id, args, false),
            None
        );
        db_a.insert_application_eval_cache(def_id, args, false, result);

        let stats = db_a.statistics();
        assert_eq!(stats.application_eval_cache_entries, 1);
        assert_eq!(stats.application_eval_cache_shared_inserts, 1);
        assert_eq!(stats.application_eval_cache_shared_hits, 0);
        assert_eq!(stats.application_eval_cache_shared_misses, 1);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_application_eval_cache(def_id, args, false),
            Some(result)
        );

        let stats = db_b.statistics();
        assert_eq!(stats.application_eval_cache_entries, 1);
        assert_eq!(stats.application_eval_cache_hits, 1);
        assert_eq!(stats.application_eval_cache_misses, 0);
        assert_eq!(stats.application_eval_cache_shared_hits, 1);
        assert_eq!(stats.application_eval_cache_shared_misses, 0);
        assert_eq!(stats.application_eval_cache_shared_inserts, 0);
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
fn application_eval_cache_invalidation_uses_recorded_def_dependencies() {
    // Structural rule: a definition-body rewrite invalidates exactly the
    // application-eval entries whose base, arguments, or result mention the
    // rewritten `DefId`. The cache records those dependencies at insertion so
    // invalidation is keyed by `DefId`, not by a full cache sweep.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let base_def = DefId(10);
    let arg_def = DefId(20);
    let result_def = DefId(30);
    let unrelated_def = DefId(40);
    let arg_ref = interner.lazy(arg_def);
    let result_ref = interner.lazy(result_def);

    db.insert_application_eval_cache(base_def, &[TypeId::STRING], false, TypeId::NUMBER);
    db.insert_application_eval_cache(DefId(11), &[arg_ref], false, TypeId::BOOLEAN);
    db.insert_application_eval_cache(DefId(12), &[TypeId::NUMBER], false, result_ref);
    db.insert_application_eval_cache(unrelated_def, &[TypeId::BOOLEAN], false, TypeId::STRING);

    assert_eq!(db.application_eval_dependency_key_count(base_def), 1);
    assert_eq!(db.application_eval_dependency_key_count(arg_def), 1);
    assert_eq!(db.application_eval_dependency_key_count(result_def), 1);
    assert_eq!(db.statistics().application_eval_cache_entries, 4);

    db.invalidate_application_eval_cache_for_def(arg_def);

    assert_eq!(
        db.lookup_application_eval_cache(DefId(11), &[arg_ref], false),
        None
    );
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[TypeId::STRING], false),
        Some(TypeId::NUMBER)
    );
    assert_eq!(
        db.lookup_application_eval_cache(DefId(12), &[TypeId::NUMBER], false),
        Some(result_ref)
    );
    assert_eq!(
        db.lookup_application_eval_cache(unrelated_def, &[TypeId::BOOLEAN], false),
        Some(TypeId::STRING)
    );
    assert_eq!(db.application_eval_dependency_key_count(arg_def), 0);
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
fn opt_in_shared_instantiation_cache_reuses_across_file_caches() {
    // Structural rule: #13240's shared instantiation cache remains disabled
    // by default, but an explicitly opted-in shared cache should let a fresh
    // file-local `QueryCache` warm itself from a sibling file's instantiation.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new_for_instantiation_family_test(true);
    let key = InstantiationCacheKey::new(TypeId::OBJECT, CanonicalSubst::empty(), 0, None);
    let result = TypeId::BOOLEAN;

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(db_a.lookup_instantiation_cache(&key), None);
        db_a.insert_instantiation_cache(key.clone(), result);

        let stats = db_a.statistics();
        assert_eq!(stats.instantiation_cache_entries, 1);
        assert_eq!(stats.instantiation_cache_shared_inserts, 1);
        assert_eq!(stats.instantiation_cache_shared_hits, 0);
        assert_eq!(stats.instantiation_cache_shared_misses, 1);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(db_b.lookup_instantiation_cache(&key), Some(result));

        let stats = db_b.statistics();
        assert_eq!(stats.instantiation_cache_entries, 1);
        assert_eq!(stats.instantiation_cache_hits, 1);
        assert_eq!(stats.instantiation_cache_misses, 0);
        assert_eq!(stats.instantiation_cache_shared_hits, 1);
        assert_eq!(stats.instantiation_cache_shared_misses, 0);
        assert_eq!(stats.instantiation_cache_shared_inserts, 0);
    }
}

// Inner relation cache inserts driven by the `SubtypeChecker`'s recursive
// descent must also populate `SharedQueryCache`, otherwise sibling per-file
// checkers re-derive the same mapped/conditional subtree relations (#10921).
// See the `SharedQueryCache` doc block for the full invariant.

#[test]
fn relation_cache_inner_inserts_are_shared_cross_file() {
    fn check(
        key: RelationCacheKey,
        result: bool,
        insert: impl Fn(&QueryCache<'_>, RelationCacheKey, bool),
        lookup: impl Fn(&QueryCache<'_>, RelationCacheKey) -> Option<bool>,
        stats: impl Fn(&QueryCacheStatistics) -> (u64, u64, usize),
    ) {
        let interner = TypeInterner::new();
        let shared = SharedQueryCache::new();

        let db_a = QueryCache::new_with_shared(&interner, &shared);
        insert(&db_a, key, result);

        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(lookup(&db_b, key), Some(result));

        // Shared hit also populates B's local cache so subsequent lookups
        // skip the `DashMap` traversal.
        assert_eq!(stats(&db_b.statistics()), (1, 0, 1));
    }

    let cfg = RelationCacheConfig::default();
    check(
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::OBJECT, cfg),
        true,
        |db, k, v| db.insert_subtype_cache(k, v),
        |db, k| db.lookup_subtype_cache(k),
        |s| {
            (
                s.relation.subtype_hits,
                s.relation.subtype_misses,
                s.relation.subtype_entries,
            )
        },
    );
    check(
        RelationCacheKey::for_assignability(TypeId::NUMBER, TypeId::UNKNOWN, cfg),
        false,
        |db, k, v| db.insert_assignability_cache(k, v),
        |db, k| db.lookup_assignability_cache(k),
        |s| {
            (
                s.relation.assignability_hits,
                s.relation.assignability_misses,
                s.relation.assignability_entries,
            )
        },
    );
}

#[test]
fn relation_cache_misses_stay_local_when_unshared() {
    // Without a `SharedQueryCache` there is no shared state to leak through;
    // file B must see only its own local cache.
    let interner = TypeInterner::new();
    let key = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::OBJECT,
        RelationCacheConfig::default(),
    );

    let db_a = QueryCache::new(&interner);
    db_a.insert_subtype_cache(key, true);

    let db_b = QueryCache::new(&interner);
    assert_eq!(db_b.lookup_subtype_cache(key), None);
    let stats_b = db_b.statistics();
    assert_eq!(stats_b.relation.subtype_hits, 0);
    assert_eq!(stats_b.relation.subtype_misses, 1);
}

#[test]
fn shared_relation_inserts_track_subtype_and_assignability_separately() {
    // Subtype and assignability live in distinct `RelationCacheKind` slots
    // even at the shared level: a subtype insert must not satisfy an
    // assignability lookup with the same `(source, target)` pair.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();
    let cfg = RelationCacheConfig::default();
    let subtype_key = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::OBJECT, cfg);
    let assignability_key =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::OBJECT, cfg);

    let db_a = QueryCache::new_with_shared(&interner, &shared);
    db_a.insert_subtype_cache(subtype_key, true);

    let db_b = QueryCache::new_with_shared(&interner, &shared);
    assert_eq!(db_b.lookup_subtype_cache(subtype_key), Some(true));
    assert_eq!(db_b.lookup_assignability_cache(assignability_key), None);
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

#[test]
fn query_cache_statistics_merge_includes_closed_eval_cache() {
    let mut left = QueryCacheStatistics {
        closed_eval_cache_entries: 2,
        ..Default::default()
    };
    let right = QueryCacheStatistics {
        closed_eval_cache_entries: 7,
        ..Default::default()
    };

    left.merge(&right);

    assert_eq!(left.closed_eval_cache_entries, 9);
}
