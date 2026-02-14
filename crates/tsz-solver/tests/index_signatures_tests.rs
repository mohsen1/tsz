use super::*;
use crate::intern::TypeInterner;
use crate::types::{ObjectFlags, ObjectShape};

#[test]
fn test_resolve_string_index() {
    let db = TypeInterner::new();

    // Object with string index
    let obj = db.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(obj), Some(TypeId::NUMBER));
    assert_eq!(resolver.resolve_number_index(obj), None);
}

#[test]
fn test_resolve_number_index() {
    let db = TypeInterner::new();

    // Object with number index
    let obj = db.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(resolver.resolve_string_index(obj), None);
    assert_eq!(resolver.resolve_number_index(obj), Some(TypeId::STRING));
}

#[test]
fn test_is_readonly() {
    let db = TypeInterner::new();

    // Readonly string index
    let obj1 = db.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
        number_index: None,
    });

    // Mutable string index
    let obj2 = db.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(resolver.is_readonly(obj1, IndexKind::String));
    assert!(!resolver.is_readonly(obj2, IndexKind::String));
}

#[test]
fn test_is_numeric_index_name() {
    let db = TypeInterner::new();
    let resolver = IndexSignatureResolver::new(&db);

    assert!(resolver.is_numeric_index_name("0"));
    assert!(resolver.is_numeric_index_name("42"));
    assert!(resolver.is_numeric_index_name("123"));
    assert!(!resolver.is_numeric_index_name("foo"));
    assert!(!resolver.is_numeric_index_name(""));
    assert!(!resolver.is_numeric_index_name("-1")); // Starts with minus
}
