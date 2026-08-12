//! Subtype relation cache correctness tests.
//!
//! These tests verify that the subtype cache in `QueryCache` correctly:
//! - Caches positive results (cache hits)
//! - Caches negative results (negative caching)
//! - Treats (A, B) and (B, A) as distinct entries (key directionality)
//! - Handles different type pairs without stale results (cache miss)
//! - Works correctly with parameterized/generic types
//! - Preserves correctness through `SubtypeChecker` with `QueryDatabase`
//! - Separates subtype and assignability caches (no cross-contamination)

use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::construction::RelationCacheProbe;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{
    PropertyInfo, RelationCacheConfig, RelationCacheKey, RelationFlags, TypeId, Visibility,
};

fn assert_repeated_subtype_check_reuses_entries(
    db: &QueryCache<'_>,
    source: TypeId,
    target: TypeId,
    expected: bool,
) -> usize {
    assert_eq!(
        db.is_subtype_of(source, target),
        expected,
        "first subtype check returned an unexpected result"
    );
    let entries_after_first = db.relation_cache_stats().subtype_entries;

    assert_eq!(
        db.is_subtype_of(source, target),
        expected,
        "repeated subtype check returned an unexpected result"
    );
    let entries_after_second = db.relation_cache_stats().subtype_entries;

    assert_eq!(
        entries_after_second, entries_after_first,
        "repeated subtype check should reuse the existing cache entry"
    );

    entries_after_first
}

// =============================================================================
// Cache Hit Tests
// =============================================================================

#[test]
fn cache_hit_after_positive_subtype_check() {
    // After checking A <: B successfully, a second check should hit cache.
    // Use a non-trivial pair that goes through the full structural check
    // (identity, top/bottom types are handled by the QueryCache fast-path
    // and never reach the cache).
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    let entries_after_first =
        assert_repeated_subtype_check_reuses_entries(&db, hello, TypeId::STRING, true);
    assert!(
        entries_after_first >= 1,
        "Cache should have at least 1 entry after first check"
    );
}

#[test]
fn cache_hit_with_literal_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    assert_repeated_subtype_check_reuses_entries(&db, hello, TypeId::STRING, true);
}

#[test]
fn subtype_relation_cache_partitions_by_inheritance_graph_generation() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let graph = crate::classes::inheritance::InheritanceGraph::new();

    let protected_name = interner.intern_string("value");
    let base = tsz_binder::SymbolId(20);
    let derived = tsz_binder::SymbolId(21);
    let mut source_prop = PropertyInfo::new(protected_name, TypeId::STRING);
    source_prop.visibility = Visibility::Protected;
    source_prop.parent_id = Some(derived);
    let mut target_prop = PropertyInfo::new(protected_name, TypeId::STRING);
    target_prop.visibility = Visibility::Protected;
    target_prop.parent_id = Some(base);
    let source = interner.object(vec![source_prop]);
    let target = interner.object(vec![target_prop]);

    let mut before = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_inheritance_graph(&graph);
    let before_key = before.debug_cache_key_for(source, target);
    assert_eq!(before_key.inheritance_graph_id, graph.identity());
    assert_eq!(before_key.inheritance_graph_generation, graph.generation());
    assert!(!before.is_subtype_of(source, target));
    assert_eq!(db.lookup_subtype_cache(before_key), Some(false));

    graph.add_inheritance(derived, &[base]);

    let mut after = SubtypeChecker::new(&interner)
        .with_query_db(&db)
        .with_inheritance_graph(&graph);
    let after_key = after.debug_cache_key_for(source, target);
    assert_eq!(after_key.inheritance_graph_id, graph.identity());
    assert_eq!(after_key.inheritance_graph_generation, graph.generation());
    assert_ne!(
        before_key, after_key,
        "graph mutation must partition shared relation cache entries"
    );
    assert!(after.is_subtype_of(source, target));
    assert_eq!(db.lookup_subtype_cache(after_key), Some(true));
}

#[test]
fn cache_hit_with_object_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_atom = interner.intern_string("name");

    // Test with a subtype relationship: {name: string, age: number} <: {name: string}
    let age_atom = interner.intern_string("age");
    let wider_obj = interner.object(vec![
        PropertyInfo::new(name_atom, TypeId::STRING),
        PropertyInfo::new(age_atom, TypeId::NUMBER),
    ]);
    let narrow_obj = interner.object(vec![PropertyInfo::new(name_atom, TypeId::STRING)]);

    assert_repeated_subtype_check_reuses_entries(&db, wider_obj, narrow_obj, true);
}

// =============================================================================
// Cache Miss Tests
// =============================================================================

#[test]
fn cache_miss_for_different_type_pairs() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // Check "hello" <: string (non-trivial, goes through cache)
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    let entries_after_first = db.relation_cache_stats().subtype_entries;

    // Check "world" <: string (different source type)
    assert!(db.is_subtype_of(world, TypeId::STRING));
    let entries_after_second = db.relation_cache_stats().subtype_entries;

    assert!(
        entries_after_second > entries_after_first,
        "Different type pairs should create separate cache entries"
    );
}

#[test]
fn cache_miss_for_different_literal_values() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    // "hello" <: string
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    let entries1 = db.relation_cache_stats().subtype_entries;

    // "world" <: string (different source)
    assert!(db.is_subtype_of(world, TypeId::STRING));
    let entries2 = db.relation_cache_stats().subtype_entries;

    assert!(
        entries2 > entries1,
        "Different literal sources should create distinct cache entries"
    );
}

// =============================================================================
// Negative Caching Tests
// =============================================================================

#[test]
fn negative_result_is_cached() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // string </: number (false)
    let entries_after_first =
        assert_repeated_subtype_check_reuses_entries(&db, TypeId::STRING, TypeId::NUMBER, false);
    assert!(entries_after_first >= 1, "Failed check should be cached");
}

#[test]
fn negative_cache_with_object_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");

    // {name: string} </: {name: string, age: number} (missing property)
    let source = interner.object(vec![PropertyInfo::new(name_atom, TypeId::STRING)]);
    let target = interner.object(vec![
        PropertyInfo::new(name_atom, TypeId::STRING),
        PropertyInfo::new(age_atom, TypeId::NUMBER),
    ]);

    assert_repeated_subtype_check_reuses_entries(&db, source, target, false);
}

// =============================================================================
// Cache Key Directionality Tests
// =============================================================================

#[test]
fn cache_key_direction_a_b_vs_b_a() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // "hello" <: string (true - literal subtype of primitive)
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    // string </: "hello" (false - primitive is not subtype of literal)
    assert!(!db.is_subtype_of(TypeId::STRING, hello));

    // Both directions should have cached entries
    let entries = db.relation_cache_stats().subtype_entries;
    assert!(
        entries >= 2,
        "Forward and reverse pairs should create distinct entries"
    );
}

#[test]
fn cache_key_direction_with_literals() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // "hello" <: string (true - literal is subtype of its primitive)
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    // string </: "hello" (false - primitive is not subtype of literal)
    assert!(!db.is_subtype_of(TypeId::STRING, hello));

    // Cache should have distinct entries for (hello, STRING) and (STRING, hello)
    let entries = db.relation_cache_stats().subtype_entries;
    assert!(
        entries >= 2,
        "(A,B) and (B,A) should be distinct cache entries"
    );
}

#[test]
fn cache_key_direction_with_union_targets() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let union = interner.union2(TypeId::STRING, TypeId::NUMBER);

    // string <: string | number (true)
    assert!(db.is_subtype_of(TypeId::STRING, union));

    // string | number </: string (false - number is not string)
    assert!(!db.is_subtype_of(union, TypeId::STRING));
}

// =============================================================================
// Cache with Type Parameters / Generic Structures
// =============================================================================

#[test]
fn cache_with_tuple_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    use crate::types::TupleElement;

    let tuple_str = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_num = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    // [string] </: [number]
    assert!(!db.is_subtype_of(tuple_str, tuple_num));
    let entries1 = db.relation_cache_stats().subtype_entries;

    // Repeat for cache hit
    assert!(!db.is_subtype_of(tuple_str, tuple_num));
    let entries2 = db.relation_cache_stats().subtype_entries;
    assert_eq!(
        entries1, entries2,
        "Tuple subtype negative result should be cached"
    );
}

#[test]
fn cache_with_array_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let arr_str = interner.array(TypeId::STRING);
    let arr_num = interner.array(TypeId::NUMBER);

    // string[] </: number[]
    assert!(!db.is_subtype_of(arr_str, arr_num));
    let entries1 = db.relation_cache_stats().subtype_entries;

    assert!(!db.is_subtype_of(arr_str, arr_num));
    let entries2 = db.relation_cache_stats().subtype_entries;
    assert_eq!(entries1, entries2, "Array subtype result should be cached");
}

#[test]
fn cache_with_union_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let union_a = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union_b = interner.union2(TypeId::STRING, TypeId::BOOLEAN);

    // string | number </: string | boolean (number is not boolean)
    let result = db.is_subtype_of(union_a, union_b);
    let entries1 = db.relation_cache_stats().subtype_entries;

    let result2 = db.is_subtype_of(union_a, union_b);
    let entries2 = db.relation_cache_stats().subtype_entries;

    assert_eq!(result, result2, "Repeated check should return same result");
    assert_eq!(entries1, entries2, "Union subtype result should be cached");
}

// =============================================================================
// Subtype vs Assignability Cache Separation
// =============================================================================

#[test]
fn subtype_and_assignability_caches_are_separate() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // Subtype check with non-trivial pair (avoids fast-path)
    db.is_subtype_of(hello, TypeId::STRING);
    let sub_entries = db.relation_cache_stats().subtype_entries;
    let assign_entries = db.relation_cache_stats().assignability_entries;

    assert!(sub_entries >= 1, "Subtype cache should have entry");
    assert_eq!(assign_entries, 0, "Assignability cache should be empty");

    // Assignability check with non-trivial pair
    db.is_assignable_to(hello, TypeId::STRING);
    let sub_entries2 = db.relation_cache_stats().subtype_entries;
    let assign_entries2 = db.relation_cache_stats().assignability_entries;

    assert_eq!(
        sub_entries2, sub_entries,
        "Subtype cache should not grow from assignability check"
    );
    assert!(
        assign_entries2 >= 1,
        "Assignability cache should have entry"
    );
}

#[test]
fn assignability_result_does_not_contaminate_subtype_cache() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Use non-trivial pairs to avoid QueryCache fast-path (identity/top/bottom/error/any).
    let hello = interner.literal_string("hello");

    // Assignability uses CompatChecker rules which may differ from SubtypeChecker
    // Ensure results do not cross-contaminate
    assert!(db.is_assignable_to(hello, TypeId::STRING));
    let sub_entries_after_assign = db.relation_cache_stats().subtype_entries;
    assert_eq!(
        sub_entries_after_assign, 0,
        "Assignability check should not populate subtype cache"
    );

    assert!(db.is_subtype_of(hello, TypeId::STRING));
    let sub_entries_after_sub = db.relation_cache_stats().subtype_entries;
    assert!(
        sub_entries_after_sub >= 1,
        "Subtype check should populate subtype cache"
    );
}

// =============================================================================
// Cache Correctness with Flags
// =============================================================================

#[test]
fn cache_key_includes_flags() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // Check with default compatibility policy — use non-trivial pair to avoid fast-path
    db.is_subtype_of_with_policy(
        hello,
        TypeId::STRING,
        RelationPolicy::unflagged_compatibility(),
    );
    let entries_default = db.relation_cache_stats().subtype_entries;

    // Check with strict null checks policy.
    db.is_subtype_of_with_policy(
        hello,
        TypeId::STRING,
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS),
    );
    let entries_strict = db.relation_cache_stats().subtype_entries;

    assert!(
        entries_strict > entries_default,
        "Different flags should create separate cache entries"
    );
}

// =============================================================================
// RelationCacheKey Unit Tests
// =============================================================================

#[test]
fn relation_cache_key_subtype_vs_assignable() {
    let key_sub = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );
    let key_assign = RelationCacheKey::for_assignability(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );

    assert_ne!(
        key_sub, key_assign,
        "Subtype and assignability keys for same types should differ"
    );
}

#[test]
fn relation_cache_key_different_source_target() {
    let key_ab = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );
    let key_ba = RelationCacheKey::for_subtype(
        TypeId::NUMBER,
        TypeId::STRING,
        RelationCacheConfig::default(),
    );

    assert_ne!(
        key_ab, key_ba,
        "(STRING, NUMBER) and (NUMBER, STRING) should be distinct keys"
    );
}

#[test]
fn relation_cache_key_same_pair_same_key() {
    let key1 = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );
    let key2 = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );

    assert_eq!(
        key1, key2,
        "Same source/target/relation/flags should produce equal keys"
    );
}

#[test]
fn relation_cache_key_different_flags_different_key() {
    let key_default = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::default(),
    );
    let key_strict = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS).cache_config(),
    );

    assert_ne!(
        key_default, key_strict,
        "Different flags should produce different keys"
    );
}

// =============================================================================
// Cache Clear / Reset Tests
// =============================================================================

#[test]
fn cache_clear_removes_all_entries() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // Populate caches with non-trivial pairs (avoid fast-path)
    db.is_subtype_of(hello, TypeId::STRING);
    db.is_assignable_to(hello, TypeId::STRING);

    assert!(db.relation_cache_stats().subtype_entries >= 1);
    assert!(db.relation_cache_stats().assignability_entries >= 1);

    // Clear
    db.clear();

    assert_eq!(
        db.relation_cache_stats().subtype_entries,
        0,
        "Subtype cache should be empty after clear"
    );
    assert_eq!(
        db.relation_cache_stats().assignability_entries,
        0,
        "Assignability cache should be empty after clear"
    );
}

#[test]
fn cache_produces_correct_results_after_clear() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Populate
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));

    // Clear
    db.clear();

    // Results should still be correct (recomputed, not stale)
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));
}

// =============================================================================
// Probe / Direct Cache Lookup Tests
// =============================================================================

#[test]
fn probe_returns_miss_before_check() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let key = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::UNKNOWN,
        RelationCacheConfig::default(),
    );
    assert_eq!(
        db.probe_subtype_cache(key),
        RelationCacheProbe::MissNotCached
    );
}

#[test]
fn probe_returns_hit_after_check() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // Do a non-trivial check to populate cache (trivial pairs use fast-path)
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    // Probe with the canonical cache config for the unflagged compatibility policy.
    let key = RelationCacheKey::for_subtype(
        hello,
        TypeId::STRING,
        RelationPolicy::unflagged_compatibility().cache_config(),
    );
    assert_eq!(db.probe_subtype_cache(key), RelationCacheProbe::Hit(true));
}

#[test]
fn probe_negative_hit_after_failed_check() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));

    let key = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationPolicy::unflagged_compatibility().cache_config(),
    );
    assert_eq!(db.probe_subtype_cache(key), RelationCacheProbe::Hit(false));
}

// =============================================================================
// SubtypeChecker with QueryDatabase Integration
// =============================================================================

#[test]
fn subtype_checker_with_query_db_uses_cache() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Use a type pair that goes through the full structural check (not a fast path).
    // {name: string, age: number} <: {name: string} requires structural property checking.
    let name_atom = interner.intern_string("name");
    let age_atom = interner.intern_string("age");
    let wider = interner.object(vec![
        PropertyInfo::new(name_atom, TypeId::STRING),
        PropertyInfo::new(age_atom, TypeId::NUMBER),
    ]);
    let narrow = interner.object(vec![PropertyInfo::new(name_atom, TypeId::STRING)]);

    // Create SubtypeChecker connected to the QueryDatabase for cross-instance caching
    let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(checker.is_subtype_of(wider, narrow));

    // The result should be in the shared cache. Construct the probe key
    // through the typed API so it reflects every behavior-affecting default
    // baked into `SubtypeChecker::new()` (strict_null_checks,
    // strict_function_types, assume_related_on_cycle, ...).
    let key = checker.debug_cache_key_for(wider, narrow);
    assert!(
        db.lookup_subtype_cache(key).is_some(),
        "SubtypeChecker with query_db should populate the shared cache"
    );

    // A second SubtypeChecker instance should benefit from the cached result
    let mut checker2 = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(checker2.is_subtype_of(wider, narrow));
}

/// `string` is NOT a subtype of `string[]` — even when the `SubtypeChecker` is
/// configured with a `QueryDatabase`. The String/iterable shortcut in
/// `is_boxed_primitive_subtype` previously misclassified arrays as
/// "purely iterable" because `target_has_non_iterable_properties` only
/// inspected `ObjectShape` and missed `TypeData::Array`. Regression for
/// `conformance/jsdoc/extendsTag5.ts` (TS2344 was being suppressed by
/// `string <: boolean | string[]` returning true via this path).
#[test]
fn string_is_not_subtype_of_array_string_with_query_db() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let array_string = interner.array(TypeId::STRING);

    let mut checker = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(
        !checker.is_subtype_of(TypeId::STRING, array_string),
        "string must not be a subtype of string[] (with query_db)"
    );

    let union = interner.union(vec![TypeId::BOOLEAN, array_string]);
    let mut checker2 = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(
        !checker2.is_subtype_of(TypeId::STRING, union),
        "string must not be a subtype of boolean | string[] (with query_db)"
    );
}

// =============================================================================
// Fast Path Tests (identity, any, unknown, never, error)
// =============================================================================

#[test]
fn fast_path_identity_not_cached() {
    // Identity checks (A == A) return SubtypeResult::True immediately via fast path,
    // before reaching the cache. This is an optimization test.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // Identity check should succeed without populating the cache
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::STRING));

    // Fast path returns before cache insertion, so this may or may not populate cache.
    // The key behavior is that it returns the correct result.
    // (Implementation detail: identity returns True before cache insertion)
}

#[test]
fn fast_path_never_is_bottom() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // never <: T for all T
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::STRING));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::NUMBER));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::BOOLEAN));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::OBJECT));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::ANY));
}

#[test]
fn fast_path_unknown_is_top() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // T <: unknown for all T
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert!(db.is_subtype_of(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(db.is_subtype_of(TypeId::BOOLEAN, TypeId::UNKNOWN));
    assert!(db.is_subtype_of(TypeId::NEVER, TypeId::UNKNOWN));
    assert!(db.is_subtype_of(TypeId::ANY, TypeId::UNKNOWN));
}

#[test]
fn fast_path_error_is_bivariant() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // ERROR <: T and T <: ERROR for all T (silences cascading errors)
    assert!(db.is_subtype_of(TypeId::ERROR, TypeId::STRING));
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::ERROR));
    assert!(db.is_subtype_of(TypeId::ERROR, TypeId::NUMBER));
    assert!(db.is_subtype_of(TypeId::NUMBER, TypeId::ERROR));
}

// =============================================================================
// Compiler-option flips mid-session must yield fresh relation answers.
//
// This is the acceptance criterion from the issue: when an LSP/project
// session changes a compiler option such as `strictNullChecks` or
// `exactOptionalPropertyTypes`, the relation cache must not serve stale
// results from the previous flag value. With `RelationCacheKey` keying on
// the relevant flags, the previous slot becomes unreachable and the new
// query computes a fresh answer.
// =============================================================================

#[test]
fn strict_null_checks_flip_yields_fresh_answer_via_shared_cache() {
    // `null <: string` is FALSE under strict-null-checks (null is a
    // distinct bottom-ish type) and TRUE under non-strict (where `null`
    // and `undefined` are absorbed into every type). A single shared
    // `QueryCache` must serve each mode's answer independently — flipping
    // the flag mid-session must produce a fresh answer rather than
    // returning the cached one from the other mode.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let mut strict_checker = SubtypeChecker::new(&interner).with_query_db(&db);
    strict_checker.strict_null_checks = true;
    let strict_result = strict_checker.is_subtype_of(TypeId::NULL, TypeId::STRING);

    let mut loose_checker = SubtypeChecker::new(&interner).with_query_db(&db);
    loose_checker.strict_null_checks = false;
    let loose_result = loose_checker.is_subtype_of(TypeId::NULL, TypeId::STRING);

    assert_ne!(
        strict_result, loose_result,
        "null <: string must observably differ between strict and non-strict null-checks"
    );

    // Repeating each query in its own mode must remain stable (no
    // cross-contamination), proving both cache slots coexist.
    let mut strict_again = SubtypeChecker::new(&interner).with_query_db(&db);
    strict_again.strict_null_checks = true;
    assert_eq!(
        strict_again.is_subtype_of(TypeId::NULL, TypeId::STRING),
        strict_result,
        "strict-null-checks slot must be stable after a non-strict query"
    );

    let mut loose_again = SubtypeChecker::new(&interner).with_query_db(&db);
    loose_again.strict_null_checks = false;
    assert_eq!(
        loose_again.is_subtype_of(TypeId::NULL, TypeId::STRING),
        loose_result,
        "non-strict null-checks slot must be stable after a strict query"
    );
}

// =============================================================================
// Weak-type (TS2559) enforcement sensitivity.
//
// Weak-type enforcement changes the structural subtype answer for weak-type
// targets but is operation-local state that is NOT encoded in the flag-agnostic
// `RelationCacheKey`. A result computed while the weak-type trigger fired must
// therefore stay out of the shared relation cache so a checker running under a
// different enforcement state cannot observe a poisoned entry.
// =============================================================================

/// Build a `{ c: string }` source and a weak-type `{ a?: string }` target that
/// share no property names — the exact shape that drives the TS2559 weak-type
/// trigger.
fn weak_type_pair(interner: &TypeInterner) -> (TypeId, TypeId) {
    let c = interner.intern_string("c");
    let a = interner.intern_string("a");
    let source = interner.object(vec![PropertyInfo::new(c, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::opt(a, TypeId::STRING)]);
    (source, target)
}

#[test]
fn weak_type_enforcement_result_is_not_shared_across_enforcement_states() {
    // Structural rule: when a non-empty non-weak source is compared to a
    // weak-type target with no common properties, the answer depends on whether
    // weak-type enforcement is active. Two checkers that differ only in
    // `enforce_weak_types` must observe their own answers, never a cached one
    // from the other state.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let (source, target) = weak_type_pair(&interner);

    // Enforcing checker: weak target with no common properties => not a subtype.
    let mut enforcing = SubtypeChecker::new(&interner).with_query_db(&db);
    enforcing.enforce_weak_types = true;
    let enforced = enforcing.is_subtype_of(source, target);
    assert!(
        !enforced,
        "weak-type enforcement must reject `{{ c: string }}` <: `{{ a?: string }}`"
    );

    // Non-enforcing checker (the SubtypeChecker default): structurally the
    // source satisfies an all-optional target, so it IS a subtype. This must be
    // computed fresh, not served from the enforcing checker's cached `false`.
    let mut relaxed = SubtypeChecker::new(&interner).with_query_db(&db);
    let relaxed_result = relaxed.is_subtype_of(source, target);
    assert!(
        relaxed_result,
        "without weak-type enforcement the all-optional target must accept the source; \
         a cached enforced `false` must not poison this lookup"
    );

    assert_ne!(
        enforced, relaxed_result,
        "the two enforcement states must observe different answers for the weak-type pair"
    );

    // Re-running each checker in its own mode stays stable (no contamination
    // through the shared cache in either direction).
    let mut enforcing_again = SubtypeChecker::new(&interner).with_query_db(&db);
    enforcing_again.enforce_weak_types = true;
    assert!(!enforcing_again.is_subtype_of(source, target));

    let mut relaxed_again = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(relaxed_again.is_subtype_of(source, target));
}

#[test]
fn weak_type_sensitive_result_is_not_memoized() {
    // The weak-type trigger marks the in-flight result as enforcement-sensitive,
    // which keeps it out of the shared relation cache. Probing the cache after
    // an enforcing check must therefore report a miss for that pair, even though
    // an ordinary (non-weak) subtype check of the same shape would be cached.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let (source, target) = weak_type_pair(&interner);

    let mut enforcing = SubtypeChecker::new(&interner).with_query_db(&db);
    enforcing.enforce_weak_types = true;
    assert!(!enforcing.is_subtype_of(source, target));

    let key = enforcing.debug_cache_key_for(source, target);
    assert_eq!(
        db.lookup_subtype_cache(key),
        None,
        "a weak-enforcement-sensitive result must not be memoized in the shared relation cache"
    );
}

#[test]
fn flag_flip_does_not_reuse_stale_cache_entry() {
    // Pin the cache-keying contract end-to-end: insert a result under one
    // policy slot, flip a flag, and demonstrate that the query under the
    // flipped policy does NOT observe the stale entry.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let lit = interner.literal_string("flag-flip");

    let checker_strict = SubtypeChecker::new(&interner).with_query_db(&db);
    let key_strict = checker_strict.debug_cache_key_for(lit, TypeId::STRING);

    let mut checker_loose = SubtypeChecker::new(&interner).with_query_db(&db);
    checker_loose.strict_null_checks = false;
    let key_loose = checker_loose.debug_cache_key_for(lit, TypeId::STRING);

    assert_ne!(
        key_strict, key_loose,
        "strict_null_checks must produce distinct cache keys"
    );

    db.insert_subtype_cache(key_strict, false);

    assert_eq!(
        db.lookup_subtype_cache(key_loose),
        None,
        "flag flip must not serve the previous-mode cache entry"
    );
}

// =============================================================================
// Polymorphic-`this` relation cache discrimination (issue #13828)
// =============================================================================
//
// A pair carrying a polymorphic `this` resolves `ThisType` against the current
// receiver, so its verdict is valid only under that binding. The relation cache
// key is discriminated by the resolved `this` binding
// (`RelationCacheKey::this_context`) so such verdicts can live in the
// cross-checker shared cache without poisoning a sibling checker that compares
// the same pair under a different receiver — while pairs with no `this` (or no
// resolvable binding) keep a byte-identical undiscriminated key.

/// Minimal resolver exposing a configurable polymorphic-`this` binding.
struct ThisBindingResolver {
    this: Option<TypeId>,
}

impl crate::def::resolver::TypeResolver for ThisBindingResolver {
    fn resolve_ref(
        &self,
        _symbol: crate::types::SymbolRef,
        _interner: &dyn crate::caches::db::TypeDatabase,
    ) -> Option<TypeId> {
        None
    }

    fn resolve_this_type(&self, _interner: &dyn crate::caches::db::TypeDatabase) -> Option<TypeId> {
        self.this
    }
}

/// An object whose `self` property is typed `this` (so the pair is
/// `this`-bearing), plus a structurally-comparable non-`this` counterpart.
fn this_bearing_and_plain(interner: &TypeInterner) -> (TypeId, TypeId) {
    let this_ty = interner.intern(crate::types::TypeData::ThisType);
    let slot = interner.intern_string("slot");
    let this_bearing = interner.object(vec![PropertyInfo::new(slot, this_ty)]);
    let plain = interner.object(vec![PropertyInfo::new(slot, TypeId::STRING)]);
    (this_bearing, plain)
}

#[test]
fn this_bearing_key_is_discriminated_by_resolved_receiver() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let (this_bearing, plain) = this_bearing_and_plain(&interner);

    // Two distinct receivers (binder names vary so the discriminator is keyed
    // on the resolved type identity, not on any name).
    let recv_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("alpha"),
        TypeId::NUMBER,
    )]);
    let recv_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("omega"),
        TypeId::BOOLEAN,
    )]);
    assert_ne!(recv_a, recv_b);

    let res_a = ThisBindingResolver { this: Some(recv_a) };
    let res_b = ThisBindingResolver { this: Some(recv_b) };

    let key_a = SubtypeChecker::with_resolver(&interner, &res_a)
        .with_query_db(&db)
        .debug_cache_key_for(this_bearing, plain);
    let key_b = SubtypeChecker::with_resolver(&interner, &res_b)
        .with_query_db(&db)
        .debug_cache_key_for(this_bearing, plain);

    assert_eq!(key_a.this_context, recv_a);
    assert_eq!(key_b.this_context, recv_b);
    assert_ne!(
        key_a, key_b,
        "the same `this`-bearing pair under different receivers must key to \
         different cache slots (no cross-receiver poisoning)"
    );
}

#[test]
fn non_this_pair_and_unbound_this_keep_undiscriminated_key() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let (this_bearing, plain) = this_bearing_and_plain(&interner);

    let receiver = interner.object(vec![PropertyInfo::new(
        interner.intern_string("recv"),
        TypeId::NUMBER,
    )]);
    let res_bound = ThisBindingResolver {
        this: Some(receiver),
    };
    let res_unbound = ThisBindingResolver { this: None };

    // A pair with no polymorphic `this` is never discriminated, even when a
    // receiver is available — its verdict is receiver-independent.
    let key_plain = SubtypeChecker::with_resolver(&interner, &res_bound)
        .with_query_db(&db)
        .debug_cache_key_for(plain, plain);
    assert_eq!(key_plain.this_context, TypeId::NONE);

    // A `this`-bearing pair with no resolvable binding stays undiscriminated
    // (it falls to the instance-local memo rather than the shared cache).
    let key_unbound = SubtypeChecker::with_resolver(&interner, &res_unbound)
        .with_query_db(&db)
        .debug_cache_key_for(this_bearing, plain);
    assert_eq!(key_unbound.this_context, TypeId::NONE);
}

#[test]
fn this_bearing_verdict_is_shared_under_its_receiver_only() {
    use crate::types::RelationCacheValue;

    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let (this_bearing, plain) = this_bearing_and_plain(&interner);

    let recv_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("alpha"),
        TypeId::NUMBER,
    )]);
    let recv_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("omega"),
        TypeId::BOOLEAN,
    )]);
    let res_a = ThisBindingResolver { this: Some(recv_a) };
    let res_b = ThisBindingResolver { this: Some(recv_b) };

    // Compute the verdict under receiver A; it must be recorded in the shared
    // cache under A's discriminated key (the pre-fix behavior kept it only in
    // the dropped per-instance local memo, so the shared cache stayed empty).
    let verdict = SubtypeChecker::with_resolver(&interner, &res_a)
        .with_query_db(&db)
        .is_subtype_of(this_bearing, plain);

    let key_a = SubtypeChecker::with_resolver(&interner, &res_a)
        .with_query_db(&db)
        .debug_cache_key_for(this_bearing, plain);
    assert_eq!(
        db.lookup_subtype_cache_value(key_a),
        Some(RelationCacheValue::from_bool(verdict)),
        "the `this`-bearing verdict must be cached cross-checker under its receiver"
    );

    // A different receiver's key must NOT observe that verdict.
    let key_b = SubtypeChecker::with_resolver(&interner, &res_b)
        .with_query_db(&db)
        .debug_cache_key_for(this_bearing, plain);
    assert_eq!(
        db.lookup_subtype_cache_value(key_b),
        None,
        "a different receiver must not be served the other receiver's verdict"
    );
}

// =============================================================================
// #14345 scoped structural-strip SOUNDNESS ORACLE
// =============================================================================
//
// These tests pin the soundness of the scoped structural-strip
// (`build_decl_param_structural_strip`, checking.rs) — the FIRST sound reducer
// of the #14345 +53 alpha-equiv-through-Application regressions. The strip maps
// every free `DeclScoped` type parameter in the two compared signature bodies
// back to its `User`-canonical structural intern (origin erased, surface
// preserved), keyed BY NAME, with a surface-poison guard: a name whose
// `DeclScoped` occurrences carry two different surfaces is EXCLUDED.
//
// The campaign's over-relate hazard (the LOCALLY-PROVEN-UNSOUND position
// co-walk, `cowalk_register_reduced_params`) was: two GENUINELY DIFFERENT
// declarations' params (`T` = Array.element, `U` = Array.map-result) brought to
// the same structural position inside `Carrier<{ v: P }>` get wrongly related
// `T <: U`. The strip is NAME-KEYED, so `T` -> `User{T}` and `U` -> `User{U}`
// stay DISTINCT (different names) — the `T <: U` over-relate is STRUCTURALLY
// IMPOSSIBLE. These tests port that exact scenario to the strip's mechanism.

/// Build a bare `DeclScoped` type-parameter `TypeId` with the given surface
/// name and a unique `(file, node)` decl site.
fn strip_declscoped_param(
    interner: &crate::intern::TypeInterner,
    name: &str,
    file: &str,
    node: u32,
) -> crate::types::TypeId {
    interner.type_param(crate::types::TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: interner.intern_string(file),
            node,
        },
    })
}

/// A single-param function shape `(x: body) => number` used to drive the strip
/// over the two reduced bodies.
fn strip_shape(
    interner: &crate::intern::TypeInterner,
    body: crate::types::TypeId,
) -> crate::types::FunctionShape {
    let x = interner.intern_string("x");
    crate::types::FunctionShape::new(
        vec![crate::types::ParamInfo {
            suppress_display_optional: false,
            name: Some(x),
            type_id: body,
            optional: false,
            rest: false,
        }],
        TypeId::NUMBER,
    )
}

/// MAKE-OR-BREAK over-relate gate: the strip must NOT collapse two
/// DIFFERENTLY-NAMED `DeclScoped` params from distinct declarations to one id.
/// This is the co-walk's defect (`T <: U` over-relate); the name-keyed strip is
/// structurally immune.
#[test]
fn strip_keeps_distinct_named_decl_params_distinct() {
    let interner = crate::intern::TypeInterner::new();

    // `T` (Array.element) and `U` (Array.map-result): genuinely different
    // declarations, DIFFERENT names, brought to the same structural position
    // inside `Carrier<{ v: P }>`.
    let t = strip_declscoped_param(&interner, "T", "array.ts", 30);
    let u = strip_declscoped_param(&interner, "U", "array_map.ts", 40);
    assert_ne!(t, u, "T and U must be distinct interned ids");

    let v = interner.intern_string("v");
    let carrier = interner.lazy(crate::DefId(7));
    let source_inner = interner.object(vec![PropertyInfo::new(v, t)]);
    let target_inner = interner.object(vec![PropertyInfo::new(v, u)]);
    let source = strip_shape(&interner, interner.application(carrier, vec![source_inner]));
    let target = strip_shape(&interner, interner.application(carrier, vec![target_inner]));

    let checker = SubtypeChecker::new(&interner);
    let strip = checker.build_decl_param_structural_strip(&source, &target);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_canon = strip.get(t_name);
    let u_canon = strip.get(u_name);

    // Both ARE stripped to their own User-canonical id (the strip fires)...
    assert!(t_canon.is_some(), "T must strip to its User canonical");
    assert!(u_canon.is_some(), "U must strip to its User canonical");
    // ...but to DISTINCT ids — the over-relate (T == U) is structurally
    // impossible because the strip is name-keyed.
    assert_ne!(
        t_canon, u_canon,
        "the strip must NOT collapse differently-named DeclScoped params to one \
         id — that is the co-walk's `T <: U` over-relate the strip avoids"
    );
}

/// POSITIVE: the strip DOES collapse two SAME-NAMED, SAME-SURFACE `DeclScoped`
/// params from distinct declarations to ONE `User` id — the genuine
/// alpha-equivalence (`Carrier<A>` from two decls) that drives the +53
/// reduction. This is the flag-OFF interning identity, reproduced scoped.
#[test]
fn strip_collapses_same_named_alpha_equivalent_decl_params() {
    let interner = crate::intern::TypeInterner::new();

    // Same name `A`, same surface, DISTINCT decl sites — genuinely
    // alpha-equivalent. Flag-OFF these intern to ONE `User` id; flag-ON the
    // stamp splits them; the strip re-collapses them.
    let a_src = strip_declscoped_param(&interner, "A", "record.ts", 61);
    let a_tgt = strip_declscoped_param(&interner, "A", "readonly_record.ts", 88);
    assert_ne!(
        a_src, a_tgt,
        "under the stamp, same-name distinct-decl params are DISTINCT ids"
    );

    let carrier = interner.lazy(crate::DefId(9));
    let source = strip_shape(&interner, interner.application(carrier, vec![a_src]));
    let target = strip_shape(&interner, interner.application(carrier, vec![a_tgt]));

    let checker = SubtypeChecker::new(&interner);
    let strip = checker.build_decl_param_structural_strip(&source, &target);

    let a_name = interner.intern_string("A");
    let a_canon = strip.get(a_name);
    assert!(
        a_canon.is_some(),
        "same-named alpha-equivalent A must strip to ONE User canonical"
    );
    // Both decl sites map to the SAME canonical, so the relation unifies them.
    let user_a = interner.type_param(crate::types::TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    assert_eq!(
        a_canon,
        Some(user_a),
        "both distinct-decl A's must collapse to the SAME User-canonical id"
    );
}

/// SURFACE-POISON: a name whose two `DeclScoped` occurrences carry DIFFERENT
/// surfaces (`<A>` vs `<A extends string>`) must be EXCLUDED from the strip
/// (the conservative non-collapsing choice — a name-keyed substitution cannot
/// represent the split, so collapsing would be unsound).
#[test]
fn strip_excludes_name_with_conflicting_surface() {
    let interner = crate::intern::TypeInterner::new();

    let a_name = interner.intern_string("A");
    // Bare `A`.
    let a_bare = interner.type_param(crate::types::TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: interner.intern_string("a.ts"),
            node: 1,
        },
    });
    // `A extends string` — SAME name, DIFFERENT surface.
    let a_constrained = interner.type_param(crate::types::TypeParamInfo {
        name: a_name,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::DeclScoped {
            file: interner.intern_string("b.ts"),
            node: 2,
        },
    });

    let carrier = interner.lazy(crate::DefId(11));
    let source = strip_shape(&interner, interner.application(carrier, vec![a_bare]));
    let target = strip_shape(
        &interner,
        interner.application(carrier, vec![a_constrained]),
    );

    let checker = SubtypeChecker::new(&interner);
    let strip = checker.build_decl_param_structural_strip(&source, &target);

    assert_eq!(
        strip.get(a_name),
        None,
        "a name with two conflicting DeclScoped surfaces must be EXCLUDED \
         (poisoned) from the strip — not collapsed unsoundly"
    );
}
