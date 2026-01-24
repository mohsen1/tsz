use crate::solver::{
    LiteralValue, QueryCache, QueryDatabase, SalsaDatabase, TypeDatabase, TypeId, TypeInterner,
    TypeKey,
};
use std::sync::Arc;

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

#[test]
fn salsa_database_implements_type_database() {
    let interner = Arc::new(TypeInterner::new());
    let db = SalsaDatabase::new(interner);

    // Test that SalsaDatabase can be used as TypeDatabase
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
fn salsa_database_query_caching() {
    let interner = Arc::new(TypeInterner::new());
    let db = SalsaDatabase::new(interner);

    // Query multiple times - Salsa should cache results
    for _ in 0..10 {
        let result = db.evaluate_type(TypeId::STRING);
        assert_eq!(result, TypeId::STRING);
    }

    // Subtype queries should also be cached
    for _ in 0..10 {
        assert!(db.is_subtype_of(TypeId::STRING, TypeId::ANY));
    }
}

#[test]
fn salsa_database_coexists_with_legacy() {
    // Test that both implementations produce the same results
    let interner1 = TypeInterner::new();
    let legacy_db: &dyn TypeDatabase = &interner1;

    let interner2 = Arc::new(TypeInterner::new());
    let salsa_db = SalsaDatabase::new(interner2);

    // Both should produce the same type ID for string literals
    let legacy_id = legacy_db.literal_string("test");
    let salsa_id = salsa_db.literal_string("test");

    // The IDs might be different but should represent the same type
    let legacy_key = legacy_db.lookup(legacy_id).unwrap();
    let salsa_key = salsa_db.lookup(salsa_id).unwrap();

    assert_eq!(legacy_key, salsa_key);
}
