//! Tests for index signature matching in subtype checking.

use super::*;
// =============================================================================
// Index Signature Subtyping Tests
// =============================================================================

#[test]
fn test_string_index_to_string_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number } <: { [key: string]: number }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_index_covariant_value() {
    let interner = TypeInterner::new();

    // { [key: string]: "hello" } <: { [key: string]: string }
    let hello = interner.literal_string("hello");

    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_index_not_subtype_incompatible_value() {
    let interner = TypeInterner::new();

    // { [key: string]: string } NOT <: { [key: string]: number }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_props_to_index_signature() {
    let interner = TypeInterner::new();

    // { foo: number, bar: number } <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("bar"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_incompatible_props_not_subtype() {
    let interner = TypeInterner::new();

    // { foo: string, bar: number } NOT <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("bar"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_with_props_to_simple_object() {
    let interner = TypeInterner::new();

    // { [key: string]: number, foo: number } <: { foo: number }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object(vec![PropertyInfo {
        name: interner.intern_string("foo"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_number_index_to_number_index() {
    let interner = TypeInterner::new();

    // { [key: number]: string } <: { [key: number]: string }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_and_number_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number, [key: number]: number } <: { [key: string]: number }
    // Number index must be subtype of string index
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_signature_with_named_property() {
    let interner = TypeInterner::new();

    // { [key: string]: number, length: number } <: { [key: string]: number, length: number }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("length"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_signature_source_property_mismatch() {
    let interner = TypeInterner::new();

    // { [key: string]: string, foo: number } NOT <: { [key: string]: string }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_number_index_signature_source_property_mismatch() {
    let interner = TypeInterner::new();

    // { [key: number]: number, "0": string } NOT <: { [key: number]: number }
    let source = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("0"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_empty_object_to_index_signature() {
    let interner = TypeInterner::new();

    // {} <: { [key: string]: number }
    let source = interner.object(vec![]);

    let target = interner.object_with_index(ObjectShape {
                flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    // Empty object satisfies any index signature (no properties to violate it)
    assert!(is_subtype_of(&interner, source, target));
}
