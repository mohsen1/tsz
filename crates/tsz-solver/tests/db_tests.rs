use crate::{
    LiteralValue, ObjectFlags, PropertyInfo, QueryCache, QueryDatabase, RelationCacheKey,
    RelationCacheProbe, TupleElement, TypeData, TypeDatabase, TypeId, TypeInterner, Visibility,
};

impl<'a> QueryCache<'a> {
    fn eval_cache_len(&self) -> usize {
        match self.eval_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    fn subtype_cache_len(&self) -> usize {
        match self.subtype_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    fn assignability_cache_len(&self) -> usize {
        match self.assignability_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    fn property_cache_len(&self) -> usize {
        match self.property_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    fn element_access_cache_len(&self) -> usize {
        match self.element_access_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
    }

    fn object_spread_properties_cache_len(&self) -> usize {
        match self.object_spread_properties_cache.read() {
            Ok(cache) => cache.len(),
            Err(e) => e.into_inner().len(),
        }
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

    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 1);
    assert_eq!(db.evaluate_type(TypeId::STRING), TypeId::STRING);
    assert_eq!(db.eval_cache_len(), 1);

    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert_eq!(db.subtype_cache_len(), 1);
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert_eq!(db.subtype_cache_len(), 1);
}

/// Test cache poisoning prevention.
///
/// CRITICAL: This test ensures that separate caches don't interfere.
/// The assignability cache (CompatChecker) and subtype cache (SubtypeChecker)
/// are kept separate to prevent cross-contamination.
///
/// For example, with sound_mode enabled:
/// - `is_subtype_of`: SubtypeChecker with configured any_propagation mode
/// - `is_assignable_to`: CompatChecker with full TypeScript rules (weak types, etc.)
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

    let key = RelationCacheKey::subtype(TypeId::STRING, TypeId::UNKNOWN, 0, 0);

    assert_eq!(
        db.probe_subtype_cache(key),
        RelationCacheProbe::MissNotCached
    );
    assert!(db.is_subtype_of(TypeId::STRING, TypeId::UNKNOWN));
    assert_eq!(db.probe_subtype_cache(key), RelationCacheProbe::Hit(true));

    let stats = db.relation_cache_stats();
    assert!(stats.subtype_hits >= 1);
    assert!(stats.subtype_misses >= 1);
    assert!(stats.subtype_entries >= 1);
}

/// Test that is_subtype_of and is_assignable_to both handle `any` correctly.
///
/// The key difference is:
/// - `is_subtype_of`: Direct SubtypeChecker - structural subtyping with any propagation
/// - `is_assignable_to`: CompatChecker - adds weak type detection, empty object rules, etc.
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
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    let second_obj = db.object_with_flags(
        vec![PropertyInfo {
            name: interner.intern_string("second"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
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
