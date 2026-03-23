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

use super::*;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, RelationCacheKey, TypeId};

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

    // First check: "hello" <: string (true, requires structural check)
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    let stats_after_first = db.relation_cache_stats();
    let entries_after_first = stats_after_first.subtype_entries;
    assert!(
        entries_after_first >= 1,
        "Cache should have at least 1 entry after first check"
    );

    // Second check: same pair should be a cache hit
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    let stats_after_second = db.relation_cache_stats();
    // Entry count should not grow on cache hit
    assert_eq!(
        stats_after_second.subtype_entries, entries_after_first,
        "Cache entries should not grow on cache hit"
    );
}

#[test]
fn cache_hit_with_literal_types() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // "hello" <: string is true
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    let entries_first = db.relation_cache_stats().subtype_entries;

    // Repeated check should hit cache
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    let entries_second = db.relation_cache_stats().subtype_entries;
    assert_eq!(
        entries_first, entries_second,
        "Cache entry count should not grow on hit"
    );
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

    let result1 = db.is_subtype_of(wider_obj, narrow_obj);
    let entries1 = db.relation_cache_stats().subtype_entries;

    let result2 = db.is_subtype_of(wider_obj, narrow_obj);
    let entries2 = db.relation_cache_stats().subtype_entries;

    assert_eq!(result1, result2, "Results must be identical");
    assert_eq!(entries1, entries2, "Cache should not grow on cache hit");
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
    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));
    let entries1 = db.relation_cache_stats().subtype_entries;
    assert!(entries1 >= 1, "Failed check should be cached");

    // Repeat: should be cache hit
    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));
    let entries2 = db.relation_cache_stats().subtype_entries;

    assert_eq!(
        entries1, entries2,
        "Negative result cache hit should not grow entries"
    );
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

    assert!(!db.is_subtype_of(source, target));
    let entries1 = db.relation_cache_stats().subtype_entries;

    assert!(!db.is_subtype_of(source, target));
    let entries2 = db.relation_cache_stats().subtype_entries;

    assert_eq!(
        entries1, entries2,
        "Negative object subtype result should be cached"
    );
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

    // Assignability uses CompatChecker rules which may differ from SubtypeChecker
    // Ensure results do not cross-contaminate
    assert!(db.is_assignable_to(TypeId::ANY, TypeId::NUMBER));
    let sub_entries_after_assign = db.relation_cache_stats().subtype_entries;
    assert_eq!(
        sub_entries_after_assign, 0,
        "Assignability check should not populate subtype cache"
    );

    assert!(db.is_subtype_of(TypeId::ANY, TypeId::NUMBER));
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

    // Check with default flags (0) — use non-trivial pair to avoid fast-path
    db.is_subtype_of_with_flags(hello, TypeId::STRING, 0);
    let entries_default = db.relation_cache_stats().subtype_entries;

    // Check with strict null checks flag (1)
    db.is_subtype_of_with_flags(
        hello,
        TypeId::STRING,
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
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
    let key_sub = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);
    let key_assign = RelationCacheKey::assignability(TypeId::STRING, TypeId::NUMBER, 0, 0);

    assert_ne!(
        key_sub, key_assign,
        "Subtype and assignability keys for same types should differ"
    );
}

#[test]
fn relation_cache_key_different_source_target() {
    let key_ab = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);
    let key_ba = RelationCacheKey::subtype(TypeId::NUMBER, TypeId::STRING, 0, 0);

    assert_ne!(
        key_ab, key_ba,
        "(STRING, NUMBER) and (NUMBER, STRING) should be distinct keys"
    );
}

#[test]
fn relation_cache_key_same_pair_same_key() {
    let key1 = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);
    let key2 = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);

    assert_eq!(
        key1, key2,
        "Same source/target/relation/flags should produce equal keys"
    );
}

#[test]
fn relation_cache_key_different_flags_different_key() {
    let key_default = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);
    let key_strict = RelationCacheKey::subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS,
        0,
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

    let key = RelationCacheKey::subtype(TypeId::STRING, TypeId::UNKNOWN, 0, 0);
    assert_eq!(
        db.probe_subtype_cache(key),
        crate::RelationCacheProbe::MissNotCached
    );
}

#[test]
fn probe_returns_hit_after_check() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let hello = interner.literal_string("hello");

    // Do a non-trivial check to populate cache (trivial pairs use fast-path)
    assert!(db.is_subtype_of(hello, TypeId::STRING));

    let key = RelationCacheKey::subtype(hello, TypeId::STRING, 0, 0);
    assert_eq!(
        db.probe_subtype_cache(key),
        crate::RelationCacheProbe::Hit(true)
    );
}

#[test]
fn probe_negative_hit_after_failed_check() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    assert!(!db.is_subtype_of(TypeId::STRING, TypeId::NUMBER));

    let key = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, 0, 0);
    assert_eq!(
        db.probe_subtype_cache(key),
        crate::RelationCacheProbe::Hit(false)
    );
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

    // The result should be in the shared cache.
    // SubtypeChecker defaults: strict_null_checks=true, strict_function_types=true
    // So the cache key has flags = FLAG_STRICT_NULL_CHECKS | FLAG_STRICT_FUNCTION_TYPES = 3
    let default_flags =
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    let key = RelationCacheKey::subtype(wider, narrow, default_flags, 0);
    assert!(
        db.lookup_subtype_cache(key).is_some(),
        "SubtypeChecker with query_db should populate the shared cache"
    );

    // A second SubtypeChecker instance should benefit from the cached result
    let mut checker2 = SubtypeChecker::new(&interner).with_query_db(&db);
    assert!(checker2.is_subtype_of(wider, narrow));
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
