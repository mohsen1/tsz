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

/// TS7017 vs TS7053 distinction: Object without index signatures should report
/// `has_index_signature` = false for both kinds (triggers TS7017 in checker).
#[test]
fn test_has_index_signature_plain_object() {
    use crate::types::PropertyInfo;

    let db = TypeInterner::new();
    let atom = db.intern_string("prop");
    let obj = db.object(vec![PropertyInfo {
        name: atom,
        type_id: TypeId::STRING,
        ..PropertyInfo::default()
    }]);

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.has_index_signature(obj, IndexKind::String),
        "plain object should have no string index signature"
    );
    assert!(
        !resolver.has_index_signature(obj, IndexKind::Number),
        "plain object should have no number index signature"
    );
}

/// `ObjectWithIndex` that has a string index signature should report true for
/// string and false for number (triggers TS7053 in checker for mismatched index type).
#[test]
fn test_has_index_signature_with_string_index() {
    let db = TypeInterner::new();
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
    assert!(
        resolver.has_index_signature(obj, IndexKind::String),
        "object with string index should report has_index_signature(String) = true"
    );
    assert!(
        !resolver.has_index_signature(obj, IndexKind::Number),
        "object with only string index should report has_index_signature(Number) = false"
    );
}

/// `ObjectWithIndex` that has both string and number index signatures should
/// report true for both kinds.
#[test]
fn test_has_index_signature_with_both_indexes() {
    let db = TypeInterner::new();
    let obj = db.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::ANY,
            readonly: false,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(resolver.has_index_signature(obj, IndexKind::String));
    assert!(resolver.has_index_signature(obj, IndexKind::Number));
}
