use crate::{LiteralValue, QueryCache, QueryDatabase, TypeDatabase, TypeId, TypeInterner, TypeKey};

#[test]
fn type_database_interns_and_looks_up() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let hello = db.literal_string("hello");
    let key = db.lookup(hello).expect("type should be interned");

    match key {
        TypeKey::Literal(LiteralValue::String(atom)) => {
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
