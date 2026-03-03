use super::*;
use crate::intern::TypeInterner;
use crate::types::{CallableShape, ObjectFlags, ObjectShape, TupleElement};

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
            param_name: None,
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
            param_name: None,
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
            param_name: None,
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
            param_name: None,
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
            param_name: None,
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
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(resolver.has_index_signature(obj, IndexKind::String));
    assert!(resolver.has_index_signature(obj, IndexKind::Number));
}

/// Callable types (class constructors) with static index signatures should
/// resolve string and number index signatures correctly.
#[test]
fn test_callable_string_index_resolution() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(callable),
        Some(TypeId::NUMBER),
        "callable with string index should resolve string index"
    );
    assert_eq!(
        resolver.resolve_number_index(callable),
        None,
        "callable with only string index should not resolve number index"
    );
}

#[test]
fn test_callable_number_index_resolution() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert_eq!(
        resolver.resolve_string_index(callable),
        None,
        "callable with only number index should not resolve string index"
    );
    assert_eq!(
        resolver.resolve_number_index(callable),
        Some(TypeId::STRING),
        "callable with number index should resolve number index"
    );
}

#[test]
fn test_callable_readonly_index_signatures() {
    let db = TypeInterner::new();

    let callable_readonly = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let callable_mutable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        resolver.is_readonly(callable_readonly, IndexKind::String),
        "readonly string index on callable should be detected"
    );
    assert!(
        resolver.is_readonly(callable_readonly, IndexKind::Number),
        "readonly number index on callable should be detected"
    );
    assert!(
        !resolver.is_readonly(callable_mutable, IndexKind::String),
        "mutable string index on callable should not be readonly"
    );
    assert!(
        !resolver.is_readonly(callable_mutable, IndexKind::Number),
        "mutable number index on callable should not be readonly"
    );
}

#[test]
fn test_callable_index_info_collection() {
    let db = TypeInterner::new();
    let callable = db.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let resolver = IndexSignatureResolver::new(&db);
    let info = resolver.get_index_info(callable);
    assert!(info.string_index.is_some(), "should have string index");
    assert!(info.number_index.is_some(), "should have number index");
    assert_eq!(
        info.string_index.as_ref().unwrap().value_type,
        TypeId::NUMBER
    );
    assert_eq!(
        info.number_index.as_ref().unwrap().value_type,
        TypeId::STRING
    );
    assert!(info.string_index.as_ref().unwrap().readonly);
    assert!(!info.number_index.as_ref().unwrap().readonly);
}

/// ReadonlyType(Tuple) should have a readonly number index signature.
/// This is the fix for `readonly [T, U, ...V[]]` types where computed
/// index access (e.g., `v[0+1] = 1`) should emit TS2542.
#[test]
fn test_readonly_tuple_has_readonly_number_index() {
    let db = TypeInterner::new();

    // Create a mutable tuple [number, number]
    let tuple = db.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    // Mutable tuple should NOT have a readonly number index
    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.is_readonly(tuple, IndexKind::Number),
        "mutable tuple should not have readonly number index"
    );

    // Wrap in ReadonlyType — should now have a readonly number index
    let readonly_tuple = db.readonly_type(tuple);
    assert!(
        resolver.is_readonly(readonly_tuple, IndexKind::Number),
        "readonly tuple should have readonly number index"
    );

    // String index should not be affected
    assert!(
        !resolver.is_readonly(readonly_tuple, IndexKind::String),
        "readonly tuple should not have readonly string index"
    );
}

/// ReadonlyType(Array) should have a readonly number index signature.
#[test]
fn test_readonly_array_has_readonly_number_index() {
    let db = TypeInterner::new();

    let array = db.array(TypeId::NUMBER);
    let readonly_array = db.readonly_type(array);

    let resolver = IndexSignatureResolver::new(&db);
    assert!(
        !resolver.is_readonly(array, IndexKind::Number),
        "mutable array should not have readonly number index"
    );
    assert!(
        resolver.is_readonly(readonly_array, IndexKind::Number),
        "readonly array should have readonly number index"
    );
}
