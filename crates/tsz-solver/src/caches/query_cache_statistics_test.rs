//! Query cache statistics and size-accounting coverage tests.

use crate::caches::db::{
    IntersectionMergeCacheEntry, QueryDatabase, TypeApplicationEvalCache, TypeDatabase,
};
use crate::caches::instantiation_cache::{CanonicalSubst, InstantiationCacheKey};
use crate::caches::query_cache::{QueryCache, SharedQueryCache};
use crate::caches::query_cache_statistics::QueryCacheStatistics;
use crate::def::{DefId, DefinitionStore};
use crate::intern::TypeInterner;
use crate::types::{RelationCacheConfig, RelationCacheKey, TypeId};

#[test]
fn query_cache_type_database_identity_is_backing_interner() {
    let interner = TypeInterner::new();
    let db_a = QueryCache::new(&interner);
    let db_b = QueryCache::new(&interner);
    assert_eq!(
        db_a.type_database_identity(),
        interner.type_database_identity()
    );
    assert_eq!(
        db_a.type_database_identity(),
        db_b.type_database_identity(),
        "wrappers around the same interner should share the same TypeId-universe identity"
    );

    let other = TypeInterner::new();
    let other_db = QueryCache::new(&other);
    assert_ne!(
        db_a.type_database_identity(),
        other_db.type_database_identity(),
        "different interners should remain distinct TypeId universes"
    );
}

#[test]
fn intersection_merge_cache_is_visible_in_statistics_and_size_estimate() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();

    assert_eq!(before.intersection_merge_cache_entries, 0);
    assert_eq!(before.intersection_merge_cache_hits, 0);
    assert_eq!(before.intersection_merge_cache_misses, 0);

    assert_eq!(db.lookup_intersection_merge(TypeId::STRING, 1), None);
    db.insert_intersection_merge(TypeId::STRING, 1, Some(TypeId::NUMBER));
    assert_eq!(
        db.lookup_intersection_merge(TypeId::STRING, 1),
        Some(IntersectionMergeCacheEntry::Merged(TypeId::NUMBER))
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
fn intersection_merge_cache_partitions_results_by_resolver_generation() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    db.insert_intersection_merge(TypeId::STRING, 1, None);
    assert_eq!(
        db.lookup_intersection_merge(TypeId::STRING, 1),
        Some(IntersectionMergeCacheEntry::NotEligible)
    );
    assert_eq!(db.lookup_intersection_merge(TypeId::STRING, 2), None);

    db.insert_intersection_merge(TypeId::STRING, 2, Some(TypeId::NUMBER));
    assert_eq!(
        db.lookup_intersection_merge(TypeId::STRING, 1),
        Some(IntersectionMergeCacheEntry::NotEligible)
    );
    assert_eq!(
        db.lookup_intersection_merge(TypeId::STRING, 2),
        Some(IntersectionMergeCacheEntry::Merged(TypeId::NUMBER))
    );

    let stats = db.statistics();
    assert_eq!(stats.intersection_merge_cache_entries, 2);
    assert_eq!(stats.intersection_merge_cache_hits, 3);
    assert_eq!(stats.intersection_merge_cache_misses, 1);
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
fn eval_cache_invalidation_uses_recorded_def_dependencies() {
    // Structural rule: persisted eval-memo entries are stable only while the
    // lazy `DefId` bodies they mention remain unchanged. A body rewrite must
    // evict entries whose key or result mentions that `DefId`, while leaving
    // unrelated eval entries resident.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let key_def = DefId(101);
    let result_def = DefId(102);
    let unrelated_key = TypeId::BOOLEAN;
    let key_ref = interner.lazy(key_def);
    let result_ref = interner.lazy(result_def);

    db.insert_eval_memo(key_ref, false, TypeId::STRING);
    db.insert_eval_memo(TypeId::NUMBER, false, result_ref);
    db.insert_eval_memo(unrelated_key, false, TypeId::STRING);

    assert_eq!(db.statistics().eval_cache_entries, 3);

    db.invalidate_application_eval_cache_for_def(key_def);

    assert_eq!(db.lookup_eval_memo(key_ref, false), None);
    assert_eq!(db.lookup_eval_memo(TypeId::NUMBER, false), Some(result_ref));
    assert_eq!(
        db.lookup_eval_memo(unrelated_key, false),
        Some(TypeId::STRING)
    );

    db.invalidate_application_eval_cache_for_def(result_def);

    assert_eq!(db.lookup_eval_memo(TypeId::NUMBER, false), None);
    assert_eq!(
        db.lookup_eval_memo(unrelated_key, false),
        Some(TypeId::STRING)
    );
}

#[test]
fn shared_eval_cache_invalidation_clears_promoted_sibling_cache() {
    // Structural rule: shared eval-cache hits are promoted into the local
    // per-file cache. The promotion must record the same `DefId` dependency
    // edges as a local write so a later body rewrite clears both the promoted
    // local copy and the shared entry for fresh sibling checkers.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new();
    let dep_def = DefId(103);
    let key = interner.lazy(dep_def);

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        db_a.insert_eval_memo(key, false, TypeId::NUMBER);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(db_b.lookup_eval_memo(key, false), Some(TypeId::NUMBER));
        db_b.invalidate_application_eval_cache_for_def(dep_def);
        assert_eq!(db_b.lookup_eval_memo(key, false), None);
    }

    {
        let db_c = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_c.lookup_eval_memo(key, false),
            None,
            "fresh sibling cache must not see a shared eval entry after invalidation"
        );
    }
}

#[test]
fn closed_eval_cache_invalidation_uses_recorded_def_dependencies() {
    // Structural rule: `closed_eval_cache` is local to one `QueryCache`, but
    // its values are still invalidated by rewritten lazy bodies when the key or
    // result closure mentions that `DefId`.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let key_def = DefId(104);
    let result_def = DefId(105);
    let key_ref = interner.lazy(key_def);
    let result_ref = interner.lazy(result_def);

    db.insert_closed_eval_cache(key_ref, false, TypeId::STRING);
    db.insert_closed_eval_cache(TypeId::NUMBER, false, result_ref);
    db.insert_closed_eval_cache(TypeId::BOOLEAN, false, TypeId::STRING);

    assert_eq!(db.statistics().closed_eval_cache_entries, 3);

    db.invalidate_application_eval_cache_for_def(result_def);

    assert_eq!(db.lookup_closed_eval_cache(TypeId::NUMBER, false), None);
    assert_eq!(
        db.lookup_closed_eval_cache(key_ref, false),
        Some(TypeId::STRING)
    );
    assert_eq!(
        db.lookup_closed_eval_cache(TypeId::BOOLEAN, false),
        Some(TypeId::STRING)
    );

    db.invalidate_application_eval_cache_for_def(key_def);
    assert_eq!(db.lookup_closed_eval_cache(key_ref, false), None);
}

#[test]
fn eval_family_invalidation_follows_definition_store_body_dependencies() {
    // Structural rule: with a shared `DefinitionStore`, an eval-family cache
    // entry keyed by `Lazy(A)` also depends on the lazy defs reachable from
    // A's published body. Rewriting B must therefore evict an entry computed
    // from A when A's body was `Lazy(B)`.
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let base_def = DefId(106);
    let body_def = DefId(107);
    let base_ref = interner.lazy(base_def);
    let body_ref = interner.lazy(body_def);
    store.set_body(base_def, body_ref);
    store.set_body_dependency_defs(base_def, [body_def]);

    let db = QueryCache::new(&interner).with_definition_store(&store);
    db.insert_eval_memo(base_ref, false, TypeId::STRING);
    db.insert_closed_eval_cache(base_ref, false, TypeId::NUMBER);
    db.insert_application_eval_cache(base_def, &[TypeId::STRING], false, TypeId::BOOLEAN);

    db.invalidate_application_eval_cache_for_def(body_def);

    assert_eq!(db.lookup_eval_memo(base_ref, false), None);
    assert_eq!(db.lookup_closed_eval_cache(base_ref, false), None);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[TypeId::STRING], false),
        None
    );
}

#[test]
fn eval_family_invalidation_follows_transitive_definition_store_body_dependencies() {
    // Structural rule: the body-dependency graph is transitive. If a cache
    // entry mentions `Lazy(A)`, A's body mentions `Lazy(B)`, and B's body
    // mentions `Lazy(C)`, rewriting C must evict the entry without a full
    // cache sweep.
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let root_def = DefId(118);
    let mid_def = DefId(119);
    let leaf_def = DefId(120);
    let unrelated_def = DefId(121);
    let root_ref = interner.lazy(root_def);
    let mid_ref = interner.lazy(mid_def);
    let leaf_ref = interner.lazy(leaf_def);
    let unrelated_ref = interner.lazy(unrelated_def);
    store.set_body(root_def, mid_ref);
    store.set_body_dependency_defs(root_def, [mid_def]);
    store.set_body(mid_def, leaf_ref);
    store.set_body_dependency_defs(mid_def, [leaf_def]);

    let db = QueryCache::new(&interner).with_definition_store(&store);
    db.insert_eval_memo(root_ref, false, TypeId::STRING);
    db.insert_closed_eval_cache(root_ref, false, TypeId::NUMBER);
    db.insert_application_eval_cache(root_def, &[TypeId::STRING], false, TypeId::BOOLEAN);
    db.insert_eval_memo(unrelated_ref, false, TypeId::STRING);

    db.invalidate_application_eval_cache_for_def(leaf_def);

    assert_eq!(db.lookup_eval_memo(root_ref, false), None);
    assert_eq!(db.lookup_closed_eval_cache(root_ref, false), None);
    assert_eq!(
        db.lookup_application_eval_cache(root_def, &[TypeId::STRING], false),
        None
    );
    assert_eq!(
        db.lookup_eval_memo(unrelated_ref, false),
        Some(TypeId::STRING),
        "transitive invalidation must preserve unrelated entries"
    );
}

#[test]
fn eval_family_invalidation_chases_store_body_deps_without_decoding_body_type_ids() {
    // Structural rule: a shared `DefinitionStore` body can be producer-arena
    // `TypeId` data. Cache invalidation must chase the recorded `DefId` body
    // dependency graph instead of decoding that body through the consumer
    // interner.
    let producer = TypeInterner::new();
    let consumer = TypeInterner::new();
    let store = DefinitionStore::new();
    let alias_def = DefId(108);
    let dep_def = DefId(109);
    let producer_body = producer.lazy(dep_def);
    store.set_body(alias_def, producer_body);
    store.set_body_dependency_defs(alias_def, [dep_def]);

    let alias_ref = consumer.lazy(alias_def);
    let db = QueryCache::new(&consumer).with_definition_store(&store);
    db.insert_eval_memo(alias_ref, false, TypeId::STRING);

    db.invalidate_application_eval_cache_for_def(dep_def);

    assert_eq!(db.lookup_eval_memo(alias_ref, false), None);
}

#[test]
fn eval_family_dependency_rewrite_removes_stale_body_dependency_edges() {
    // Structural rule: dependency indexes describe the cache entry that was
    // actually inserted, not the DefinitionStore graph as it happens to look
    // at removal time. If A's body-deps change from B to C, evicting the old
    // `Lazy(A)` entry must remove its stale B reverse edge before a fresh entry
    // records the new C edge.
    let interner = TypeInterner::new();
    let store = DefinitionStore::new();
    let root_def = DefId(122);
    let stale_dep = DefId(123);
    let current_dep = DefId(124);
    let root_ref = interner.lazy(root_def);
    let stale_ref = interner.lazy(stale_dep);
    let current_ref = interner.lazy(current_dep);
    store.set_body(root_def, stale_ref);
    store.set_body_dependency_defs(root_def, [stale_dep]);

    let db = QueryCache::new(&interner).with_definition_store(&store);
    db.insert_eval_memo(root_ref, false, TypeId::STRING);
    db.insert_closed_eval_cache(root_ref, false, TypeId::NUMBER);
    db.insert_application_eval_cache(root_def, &[TypeId::STRING], false, TypeId::BOOLEAN);

    store.set_body(root_def, current_ref);
    store.set_body_dependency_defs(root_def, [current_dep]);
    db.invalidate_application_eval_cache_for_def(root_def);

    db.insert_eval_memo(root_ref, false, TypeId::STRING);
    db.insert_closed_eval_cache(root_ref, false, TypeId::NUMBER);
    db.insert_application_eval_cache(root_def, &[TypeId::STRING], false, TypeId::BOOLEAN);

    db.invalidate_application_eval_cache_for_def(stale_dep);
    assert_eq!(
        db.lookup_eval_memo(root_ref, false),
        Some(TypeId::STRING),
        "stale body-dep edge must not evict the fresh eval entry"
    );
    assert_eq!(
        db.lookup_closed_eval_cache(root_ref, false),
        Some(TypeId::NUMBER),
        "stale body-dep edge must not evict the fresh closed-eval entry"
    );
    assert_eq!(
        db.lookup_application_eval_cache(root_def, &[TypeId::STRING], false),
        Some(TypeId::BOOLEAN),
        "stale body-dep edge must not evict the fresh application-eval entry"
    );

    db.invalidate_application_eval_cache_for_def(current_dep);
    assert_eq!(db.lookup_eval_memo(root_ref, false), None);
    assert_eq!(db.lookup_closed_eval_cache(root_ref, false), None);
    assert_eq!(
        db.lookup_application_eval_cache(root_def, &[TypeId::STRING], false),
        None
    );
}

#[test]
fn conditional_branch_verdict_cache_round_trips_and_is_key_partitioned() {
    // Structural rule (issues #8356 / #13097): the conditional-branch verdict
    // cache is a per-`QueryCache` map keyed by
    // `(check, extends, no_unchecked_indexed_access,
    // exact_optional_property_types)`. It stores `bool` verdicts (a distinct
    // relation from plain subtyping), is partitioned by both option flags, and
    // is cleared with the rest of the query cache. Raw-interner backends opt
    // out (default `None`/no-op).
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();
    assert_eq!(before.conditional_branch_verdict_cache_entries, 0);

    // Miss on an empty cache.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        None
    );

    // Round-trip a `true` verdict.
    db.insert_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false, true);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
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
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, true, false),
        None
    );
    // `exactOptionalPropertyTypes` also partitions the key. A branch probe can
    // depend on mapped/indexed access semantics that distinguish optional
    // markers from explicit `| undefined`, so reusing the same pair across
    // exact-optional modes would select the wrong branch.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true),
        None
    );
    db.insert_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true, false);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true),
        Some(false)
    );
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        Some(true)
    );
    // Operand order matters — `check`/`extends` are not symmetric.
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false),
        None
    );

    // A `false` verdict round-trips distinctly from an absent entry.
    db.insert_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false, false);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false),
        Some(false)
    );

    // Cleared with the rest of the query cache.
    db.clear();
    assert_eq!(db.statistics().conditional_branch_verdict_cache_entries, 0);
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        None
    );
    assert_eq!(
        db.lookup_conditional_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false),
        None
    );
}

#[test]
fn conditional_branch_verdict_cache_defaults_off_for_raw_interner() {
    // The trait default is a no-op so raw `TypeInterner` backends and tests
    // opt out: a lookup always misses and an insert is dropped.
    let interner = TypeInterner::new();
    interner.insert_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false, true);
    assert_eq!(
        interner.lookup_conditional_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        None
    );
}

#[test]
fn permissive_false_branch_cache_round_trips_and_is_key_partitioned() {
    // Structural rule (#14351): the permissive-instantiation false-branch
    // wrapper cache is keyed by the original `(check, extends)` operands plus
    // both compiler option bits. It is cleared with the `QueryCache` and is
    // distinct from the instantiated conditional-branch verdict cache that
    // certifies publication safety.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let before = db.statistics();
    assert_eq!(before.permissive_false_branch_cache_entries, 0);

    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        None
    );

    db.insert_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false, true);
    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        Some(true)
    );

    let after = db.statistics();
    assert_eq!(after.permissive_false_branch_cache_entries, 1);
    assert!(after.estimated_size_bytes() > before.estimated_size_bytes());
    assert!(after.to_string().contains("permissive_false_branch"));

    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, true, false),
        None
    );
    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true),
        None
    );

    db.insert_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true, false);
    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, true),
        Some(false)
    );
    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::NUMBER, TypeId::STRING, false, false),
        None
    );

    db.clear();
    assert_eq!(db.statistics().permissive_false_branch_cache_entries, 0);
    assert_eq!(
        db.lookup_permissive_false_branch_verdict(TypeId::STRING, TypeId::NUMBER, false, false),
        None
    );
}

#[test]
fn permissive_false_branch_cache_defaults_off_for_raw_interner() {
    let interner = TypeInterner::new();
    interner.insert_permissive_false_branch_verdict(
        TypeId::STRING,
        TypeId::NUMBER,
        false,
        false,
        true,
    );
    assert_eq!(
        interner.lookup_permissive_false_branch_verdict(
            TypeId::STRING,
            TypeId::NUMBER,
            false,
            false,
        ),
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
fn application_eval_cache_overwrite_replaces_recorded_result_dependencies() {
    // Structural rule: the dependency index describes the cache entry that is
    // currently stored for a key. Re-inserting the same application-eval key
    // with a new result must detach the key from the old result's `DefId`
    // bucket, or an invalidation of the old body would evict a live result.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let base_def = DefId(50);
    let stale_result_def = DefId(60);
    let current_result_def = DefId(70);
    let stale_result = interner.lazy(stale_result_def);
    let current_result = interner.lazy(current_result_def);

    db.insert_application_eval_cache(base_def, &[TypeId::STRING], false, stale_result);
    db.insert_application_eval_cache(base_def, &[TypeId::STRING], false, current_result);

    assert_eq!(db.application_eval_dependency_key_count(base_def), 1);
    assert_eq!(
        db.application_eval_dependency_key_count(stale_result_def),
        0,
        "overwriting a key must remove dependency edges for the old result"
    );
    assert_eq!(
        db.application_eval_dependency_key_count(current_result_def),
        1
    );

    db.invalidate_application_eval_cache_for_def(stale_result_def);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[TypeId::STRING], false),
        Some(current_result),
        "invalidating a stale result dependency must not evict the replacement"
    );

    db.invalidate_application_eval_cache_for_def(current_result_def);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[TypeId::STRING], false),
        None,
        "invalidating the current result dependency must evict the entry"
    );
    assert_eq!(
        db.application_eval_dependency_key_count(base_def),
        0,
        "eviction by result dependency must remove the base-def reverse edge"
    );
}

#[test]
fn application_eval_cache_overwrite_replaces_key_arg_and_result_dependencies() {
    // Structural rule: overwriting an application-eval key must leave exactly
    // the current key/result dependency set behind. Stale argument/result edges
    // must not evict a live replacement, while current argument/result rewrites
    // must evict it.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let base_def = DefId(110);
    let stale_arg_def = DefId(111);
    let current_arg_def = DefId(112);
    let stale_result_def = DefId(113);
    let current_result_def = DefId(114);
    let stale_arg = interner.lazy(stale_arg_def);
    let current_arg = interner.lazy(current_arg_def);
    let stale_result = interner.lazy(stale_result_def);
    let current_result = interner.lazy(current_result_def);

    db.insert_application_eval_cache(base_def, &[stale_arg], false, stale_result);
    db.insert_application_eval_cache(base_def, &[current_arg], false, current_result);

    db.invalidate_application_eval_cache_for_def(stale_arg_def);
    db.invalidate_application_eval_cache_for_def(stale_result_def);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[current_arg], false),
        Some(current_result),
        "stale argument/result dependencies must not evict a replacement key"
    );

    db.invalidate_application_eval_cache_for_def(current_arg_def);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[current_arg], false),
        None,
        "current argument dependency must evict the replacement"
    );

    db.insert_application_eval_cache(base_def, &[current_arg], false, current_result);
    db.invalidate_application_eval_cache_for_def(current_result_def);
    assert_eq!(
        db.lookup_application_eval_cache(base_def, &[current_arg], false),
        None,
        "current result dependency must evict the replacement"
    );
}

#[test]
fn shared_application_eval_cache_overwrite_replaces_recorded_result_dependencies() {
    // Structural rule: the opt-in shared application-eval cache has the same
    // exact-dependency invariant as the file-local cache. A stale dependency
    // edge in the shared index can otherwise evict the current value for fresh
    // sibling file checkers.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new_for_instantiation_family_test(true);

    let base_def = DefId(80);
    let stale_result_def = DefId(90);
    let current_result_def = DefId(100);
    let stale_result = interner.lazy(stale_result_def);
    let current_result = interner.lazy(current_result_def);

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        db_a.insert_application_eval_cache(base_def, &[TypeId::NUMBER], false, stale_result);
        db_a.insert_application_eval_cache(base_def, &[TypeId::NUMBER], false, current_result);
        db_a.invalidate_application_eval_cache_for_def(stale_result_def);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_application_eval_cache(base_def, &[TypeId::NUMBER], false),
            Some(current_result),
            "invalidating a stale shared dependency must not evict the replacement"
        );
    }

    {
        let db_c = QueryCache::new_with_shared(&interner, &shared);
        db_c.invalidate_application_eval_cache_for_def(current_result_def);
    }
    {
        let db_d = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_d.lookup_application_eval_cache(base_def, &[TypeId::NUMBER], false),
            None,
            "invalidating the current shared dependency must evict the entry"
        );
    }
}

#[test]
fn shared_application_eval_cache_promotion_records_local_dependency_edges() {
    // Structural rule: a shared application-eval hit promoted into a sibling
    // local cache must record local dependency edges. Rewriting the def from
    // that sibling then clears both its promoted local copy and the shared
    // entry for fresh siblings.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new_for_instantiation_family_test(true);
    let base_def = DefId(115);
    let dep_def = DefId(116);
    let dep_ref = interner.lazy(dep_def);

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        db_a.insert_application_eval_cache(base_def, &[dep_ref], false, TypeId::STRING);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_application_eval_cache(base_def, &[dep_ref], false),
            Some(TypeId::STRING)
        );
        db_b.invalidate_application_eval_cache_for_def(dep_def);
        assert_eq!(
            db_b.lookup_application_eval_cache(base_def, &[dep_ref], false),
            None
        );
    }

    {
        let db_c = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_c.lookup_application_eval_cache(base_def, &[dep_ref], false),
            None,
            "fresh sibling must not see the shared application-eval entry after invalidation"
        );
    }
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

#[test]
fn opt_in_shared_instantiation_cache_keeps_unstable_results_file_local() {
    // Structural rule: even when the experimental shared instantiation family
    // is enabled, results produced under tainted ambient request state are
    // local-cache facts only. The writer can reuse them in the same
    // `QueryCache`, but a sibling file-local cache must miss and recompute.
    let interner = TypeInterner::new();
    let shared = SharedQueryCache::new_for_instantiation_family_test(true);
    let key = InstantiationCacheKey::new(TypeId::OBJECT, CanonicalSubst::empty(), 0, None);
    let result = TypeId::BOOLEAN;

    {
        let db_a = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(db_a.lookup_instantiation_cache(&key), None);
        db_a.insert_instantiation_cache_with_project_stability(key.clone(), result, false);
        assert_eq!(db_a.lookup_instantiation_cache(&key), Some(result));

        let stats = db_a.statistics();
        assert_eq!(stats.instantiation_cache_entries, 1);
        assert_eq!(stats.instantiation_cache_hits, 1);
        assert_eq!(stats.instantiation_cache_misses, 1);
        assert_eq!(stats.instantiation_cache_shared_inserts, 0);
        assert_eq!(stats.instantiation_cache_shared_misses, 1);
    }

    {
        let db_b = QueryCache::new_with_shared(&interner, &shared);
        assert_eq!(
            db_b.lookup_instantiation_cache(&key),
            None,
            "unstable instantiation results must not be promoted to the shared cache"
        );
        let stats = db_b.statistics();
        assert_eq!(stats.instantiation_cache_entries, 0);
        assert_eq!(stats.instantiation_cache_hits, 0);
        assert_eq!(stats.instantiation_cache_misses, 1);
        assert_eq!(stats.instantiation_cache_shared_hits, 0);
        assert_eq!(stats.instantiation_cache_shared_misses, 1);
    }

    assert_eq!(
        shared.total_entries(),
        0,
        "shared cache should remain empty when only unstable instantiation results were inserted"
    );
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
        permissive_false_branch_cache_entries: 3,
        ..Default::default()
    };
    let right = QueryCacheStatistics {
        closed_eval_cache_entries: 7,
        permissive_false_branch_cache_entries: 11,
        ..Default::default()
    };

    left.merge(&right);

    assert_eq!(left.closed_eval_cache_entries, 9);
    assert_eq!(left.permissive_false_branch_cache_entries, 14);
}

#[test]
fn evict_registration_window_entries_drops_only_tainted_keys() {
    use crate::evaluation::request::EvaluationCacheKey;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // A clean top-level entry (would pass `is_stable_for_run_wide_cache`) and a
    // registration-window entry (passed only the depth-agnostic gate) share the
    // reused pool cache. Only the latter is window-scoped.
    let clean = EvaluationCacheKey::new(TypeId::STRING, false, false);
    let tainted = EvaluationCacheKey::new(TypeId::NUMBER, false, false);
    db.insert_eval_entry_for_test(clean, TypeId::BOOLEAN, false);
    db.insert_eval_entry_for_test(tainted, TypeId::BOOLEAN, true);
    assert!(db.eval_cache_contains_key_for_test(&clean));
    assert!(db.eval_cache_contains_key_for_test(&tainted));

    // A file boundary in the checker-pool reuse loop: the window ends, so the
    // window-scoped entry must go while the clean cross-file entry stays.
    db.evict_registration_window_eval_entries();
    assert!(
        db.eval_cache_contains_key_for_test(&clean),
        "clean cross-file entry must be retained for pool amortization"
    );
    assert!(
        !db.eval_cache_contains_key_for_test(&tainted),
        "registration-window entry must not survive into the next file's window"
    );

    // Idempotent: the tracked-key set was drained, so a second eviction is a
    // no-op and leaves the clean entry alone.
    db.evict_registration_window_eval_entries();
    assert!(db.eval_cache_contains_key_for_test(&clean));
}
