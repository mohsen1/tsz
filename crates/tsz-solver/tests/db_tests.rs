use crate::{
    LiteralValue, ObjectFlags, PropertyInfo, QueryCache, QueryCacheStatistics, QueryDatabase,
    RelationCacheKey, RelationCacheProbe, TupleElement, TypeData, TypeDatabase, TypeId,
    TypeInterner, Visibility,
};

impl<'a> QueryCache<'a> {
    fn eval_cache_len(&self) -> usize {
        self.eval_cache.borrow().len()
    }

    fn subtype_cache_len(&self) -> usize {
        self.subtype_cache.borrow().len()
    }

    fn assignability_cache_len(&self) -> usize {
        self.assignability_cache.borrow().len()
    }

    fn property_cache_len(&self) -> usize {
        self.property_cache.borrow().len()
    }

    fn element_access_cache_len(&self) -> usize {
        self.element_access_cache.borrow().len()
    }

    fn object_spread_properties_cache_len(&self) -> usize {
        self.object_spread_properties_cache.borrow().len()
    }
}

#[test]
fn type_database_interns_and_looks_up() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let hello = db.literal_string("hello");
    let key = db.lookup(hello).expect("type should be interned");

    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(db.resolve_atom(atom), "hello");
            assert_eq!(db.resolve_atom_ref(atom).as_ref(), "hello");
        }
        _ => panic!("expected string literal type"),
    }
}

#[test]
fn type_database_union_normalizes() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let union = db.union(vec![TypeId::STRING]);
    assert_eq!(union, TypeId::STRING);
}

#[test]
fn query_cache_caches_evaluate_and_subtype() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.subtype_cache_len(), 0);

    // Intrinsic types bypass the eval_cache entirely (fast path optimization).
    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 0);
    assert_eq!(db.property_cache_len(), 0);

    // Use a non-trivial pair for subtype caching: identity/top/bottom/error pairs
    // are now handled by the QueryCache fast-path and never reach the cache.
    let hello = interner.literal_string("hello");
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.subtype_cache_len(), 1);
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.subtype_cache_len(), 1);
}

/// Test cache poisoning prevention.
///
/// CRITICAL: This test ensures that separate caches don't interfere.
/// The assignability cache (`CompatChecker`) and subtype cache (`SubtypeChecker`)
/// are kept separate to prevent cross-contamination.
///
/// For example, with `sound_mode` enabled:
/// - `is_subtype_of`: `SubtypeChecker` with configured `any_propagation` mode
/// - `is_assignable_to`: `CompatChecker` with full TypeScript rules (weak types, etc.)
///
/// Even though both may return similar results for basic `any` checks,
/// the caches must be separate because they can diverge in complex cases.
#[test]
fn test_cache_poisoning_prevention() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // 1. Check assignability - uses CompatChecker with TS rules
    assert!(db.is_assignable_to(TypeId::ANY, TypeId::NUMBER));
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 0);

    // 2. Check subtype - uses SubtypeChecker (also handles any propagation)
    assert!(db.is_subtype_of(TypeId::ANY, TypeId::NUMBER));
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 1);

    // 3. Verify caches are separate - both have 1 entry proving they're independent
    assert!(db.is_assignable_to(TypeId::ANY, TypeId::NUMBER)); // Cache hit
    assert!(db.is_subtype_of(TypeId::ANY, TypeId::NUMBER)); // Cache hit

    // Check cache hit (no growth)
    assert_eq!(db.assignability_cache_len(), 1);
    assert_eq!(db.subtype_cache_len(), 1);
}

#[test]
fn relation_cache_stats_track_hits_and_misses() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    db.reset_relation_cache_stats();

    // Use non-trivial pair to avoid QueryCache fast-path (identity/top/bottom/error).
    let hello = interner.literal_string("hello");
    let key = RelationCacheKey::subtype(hello, TypeId::STRING, 0, 0);

    assert_eq!(
        db.probe_subtype_cache(key),
        RelationCacheProbe::MissNotCached
    );
    assert!(db.is_subtype_of(hello, TypeId::STRING));
    assert_eq!(db.probe_subtype_cache(key), RelationCacheProbe::Hit(true));

    let stats = db.relation_cache_stats();
    assert!(stats.subtype_hits >= 1);
    assert!(stats.subtype_misses >= 1);
    assert!(stats.subtype_entries >= 1);
}

/// Test that `is_subtype_of` and `is_assignable_to` both handle `any` correctly.
///
/// The key difference is:
/// - `is_subtype_of`: Direct `SubtypeChecker` - structural subtyping with any propagation
/// - `is_assignable_to`: `CompatChecker` - adds weak type detection, empty object rules, etc.
///
/// For basic `any` checks, both return true (TypeScript compatibility).
#[test]
fn test_is_subtype_vs_is_assignable_any() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    // For `any`, both methods handle any propagation:
    // - is_subtype_of: any is subtype of everything (SubtypeChecker)
    // - is_assignable_to: any is assignable to everything (CompatChecker)

    assert!(db.is_subtype_of(TypeId::ANY, TypeId::NUMBER));
    assert!(db.is_assignable_to(TypeId::ANY, TypeId::NUMBER));

    // Symmetric check
    assert!(db.is_subtype_of(TypeId::NUMBER, TypeId::ANY));
    assert!(db.is_assignable_to(TypeId::NUMBER, TypeId::ANY));
}

#[test]
fn query_cache_caches_element_access_type() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let tuple_type = db.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    assert_eq!(db.element_access_cache_len(), 0);
    let first = db.resolve_element_access_type(tuple_type, interner.literal_number(0.0), Some(0));
    assert_eq!(first, TypeId::STRING);
    assert_eq!(db.element_access_cache_len(), 1);

    let second = db.resolve_element_access_type(tuple_type, interner.literal_number(0.0), Some(0));
    assert_eq!(second, first);
    assert_eq!(db.element_access_cache_len(), 1);
}

#[test]
fn query_cache_caches_object_spread_properties() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let first_obj = db.object(vec![PropertyInfo {
        name: interner.intern_string("first"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
    }]);

    let second_obj = db.object_with_flags(
        vec![PropertyInfo {
            name: interner.intern_string("second"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }],
        ObjectFlags::FRESH_LITERAL,
    );

    let spread_type = db.intersection(vec![first_obj, second_obj]);
    assert_eq!(db.object_spread_properties_cache_len(), 0);

    let props = db.collect_object_spread_properties(spread_type);
    assert_eq!(props.len(), 2);
    assert!(
        props
            .iter()
            .any(|p| interner.resolve_atom_ref(p.name).as_ref() == "first")
    );
    assert!(
        props
            .iter()
            .any(|p| interner.resolve_atom_ref(p.name).as_ref() == "second")
    );
    assert_eq!(db.object_spread_properties_cache_len(), 1);

    let props_again = db.collect_object_spread_properties(spread_type);
    assert_eq!(props_again.len(), 2);
    assert_eq!(db.object_spread_properties_cache_len(), 1);
}

#[test]
fn type_interner_query_db_tracks_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;

    assert!(!db.no_unchecked_indexed_access());
    db.set_no_unchecked_indexed_access(true);
    assert!(db.no_unchecked_indexed_access());
    db.set_no_unchecked_indexed_access(false);
    assert!(!db.no_unchecked_indexed_access());
}

#[test]
fn type_interner_element_access_respects_no_unchecked_indexed_access() {
    let interner = TypeInterner::new();
    let db: &dyn QueryDatabase = &interner;

    let array = interner.array(TypeId::STRING);
    let without_flag = db.resolve_element_access_type(array, TypeId::NUMBER, None);
    assert_eq!(without_flag, TypeId::STRING);

    db.set_no_unchecked_indexed_access(true);
    let with_flag = db.resolve_element_access_type(array, TypeId::NUMBER, None);
    assert_ne!(with_flag, TypeId::STRING);
    assert!(crate::type_contains_undefined(&interner, with_flag));
}

#[test]
fn query_cache_statistics_reflects_cache_population() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Empty cache should have zero entries everywhere.
    let stats = cache.statistics();
    assert_eq!(stats, QueryCacheStatistics::default());

    // Use non-trivial pairs to avoid QueryCache fast-path (identity/top/bottom/error).
    let hello = interner.literal_string("hello");

    // Subtype check populates the subtype cache.
    let _ = cache.is_subtype_of(hello, TypeId::STRING);

    // Assignability check populates the assignability cache.
    let _ = cache.is_assignable_to(TypeId::STRING, TypeId::ANY);

    let stats = cache.statistics();
    // Relation caches should have entries from the checks above.
    assert!(
        stats.relation.subtype_entries >= 1,
        "subtype cache should be populated: {}",
        stats.relation.subtype_entries,
    );
    assert!(
        stats.relation.assignability_entries >= 1,
        "assignability cache should be populated: {}",
        stats.relation.assignability_entries,
    );
    // Display impl should not panic.
    let display_output = format!("{stats}");
    assert!(display_output.contains("QueryCache statistics:"));
    assert!(display_output.contains("eval_cache:"));
    assert!(display_output.contains("subtype_cache:"));
    assert!(display_output.contains("assignability_cache:"));
    assert!(display_output.contains("estimated_size:"));
}

#[test]
fn query_cache_estimated_size_bytes_empty() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Empty cache should still have nonzero size (Self struct)
    let size = cache.estimated_size_bytes();
    assert!(
        size > 0,
        "empty QueryCache should have nonzero estimated size"
    );
    assert!(
        size < 4096,
        "empty QueryCache should be small, got {size} bytes"
    );

    // Statistics-based estimate should be zero for empty caches
    let stats = cache.statistics();
    assert_eq!(stats.estimated_size_bytes(), 0);
}

#[test]
fn query_cache_estimated_size_grows_with_entries() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    let empty_size = cache.estimated_size_bytes();

    // Add some eval cache entries
    let str_type = interner.literal_string("hello");
    let num_type = interner.literal_number(42.0);
    cache.evaluate_type(str_type);
    cache.evaluate_type(num_type);

    // Add subtype cache entries
    cache.insert_subtype_cache(RelationCacheKey::subtype(str_type, num_type, 0, 0), false);

    // Add assignability cache entries
    cache.insert_assignability_cache(
        RelationCacheKey::assignability(str_type, num_type, 0, 0),
        false,
    );

    let populated_size = cache.estimated_size_bytes();
    assert!(
        populated_size > empty_size,
        "populated cache ({populated_size}) should be larger than empty ({empty_size})"
    );

    // Statistics snapshot should also show nonzero estimated size
    let stats = cache.statistics();
    assert!(
        stats.estimated_size_bytes() > 0,
        "statistics estimated_size_bytes should be nonzero after populating caches"
    );
}

#[test]
fn query_cache_estimated_size_resets_on_clear() {
    let interner = TypeInterner::new();
    let cache = QueryCache::new(&interner);

    // Populate
    let str_type = interner.literal_string("test");
    cache.evaluate_type(str_type);
    cache.insert_subtype_cache(
        RelationCacheKey::subtype(str_type, TypeId::NUMBER, 0, 0),
        true,
    );

    let before_clear = cache.estimated_size_bytes();

    cache.clear();

    let after_clear = cache.estimated_size_bytes();
    // After clear, size should not exceed before_clear (maps may retain capacity).
    // The key invariant: statistics-based estimate resets to zero.
    let stats = cache.statistics();
    assert_eq!(
        stats.estimated_size_bytes(),
        0,
        "statistics estimated_size_bytes should be 0 after clear"
    );
    // Live estimate may retain capacity but should be reasonable
    assert!(
        after_clear <= before_clear,
        "live estimate should not grow after clear ({after_clear} vs {before_clear})"
    );
}

#[test]
fn query_cache_statistics_merge_preserves_estimated_size() {
    let mut stats_a = QueryCacheStatistics {
        eval_cache_entries: 10,
        property_cache_entries: 5,
        ..Default::default()
    };
    let stats_b = QueryCacheStatistics {
        eval_cache_entries: 20,
        property_cache_entries: 15,
        ..Default::default()
    };

    let size_a = stats_a.estimated_size_bytes();
    let size_b = stats_b.estimated_size_bytes();

    stats_a.merge(&stats_b);

    let merged_size = stats_a.estimated_size_bytes();
    assert_eq!(
        merged_size,
        size_a + size_b,
        "merged estimated_size_bytes should equal sum of parts"
    );
}
