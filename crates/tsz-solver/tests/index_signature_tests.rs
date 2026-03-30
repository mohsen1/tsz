//! Tests for index signature matching in subtype checking.

use super::*;
use crate::TypeInterner;
// =============================================================================
// Index Signature Subtyping Tests
// =============================================================================

#[test]
fn test_string_index_to_string_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number } <: { [key: string]: number }
    let source = interner.object_with_index(ObjectShape {
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

    let target = interner.object_with_index(ObjectShape {
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

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_index_covariant_value() {
    let interner = TypeInterner::new();

    // { [key: string]: "hello" } <: { [key: string]: string }
    let hello = interner.literal_string("hello");

    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: hello,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
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
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
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

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_props_to_index_signature() {
    let interner = TypeInterner::new();

    // { foo: number, bar: number } <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::NUMBER),
    ]);

    let target = interner.object_with_index(ObjectShape {
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

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_object_with_incompatible_props_not_subtype() {
    let interner = TypeInterner::new();

    // { foo: string, bar: number } NOT <: { [key: string]: number }
    let source = interner.object(vec![
        PropertyInfo::new(interner.intern_string("foo"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("bar"), TypeId::NUMBER),
    ]);

    let target = interner.object_with_index(ObjectShape {
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

    assert!(!is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_with_props_to_simple_object() {
    let interner = TypeInterner::new();

    // { [key: string]: number, foo: number } <: { foo: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("foo"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object(vec![PropertyInfo::new(
        interner.intern_string("foo"),
        TypeId::NUMBER,
    )]);

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_number_index_to_number_index() {
    let interner = TypeInterner::new();

    // { [key: number]: string } <: { [key: number]: string }
    let source = interner.object_with_index(ObjectShape {
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

    let target = interner.object_with_index(ObjectShape {
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

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_string_and_number_index() {
    let interner = TypeInterner::new();

    // { [key: string]: number, [key: number]: number } <: { [key: string]: number }
    // Number index must be subtype of string index
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
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

    assert!(is_subtype_of(&interner, source, target));
}

#[test]
fn test_index_signature_with_named_property() {
    let interner = TypeInterner::new();

    // { [key: string]: number, length: number } <: { [key: string]: number, length: number }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("length"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("length"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
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
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("foo"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
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
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("0"),
            TypeId::STRING,
        )],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
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

    // Empty object satisfies any index signature (no properties to violate it)
    assert!(is_subtype_of(&interner, source, target));
}

// =============================================================================
// classify_element_indexable: Union Preservation Tests
// =============================================================================

/// Verify that `classify_element_indexable` returns Union for union types,
/// even when one union member is structurally a subtype of another.
///
/// Regression test: `evaluate_type`'s union simplification was collapsing
/// `{ a: number } | { [s: string]: number }` into just the `ObjectWithIndex`
/// member, because the first member is a structural subtype. This broke
/// TS7053 detection which needs per-constituent indexability information.
#[test]
fn test_classify_element_indexable_preserves_union_members() {
    use crate::type_queries::{ElementIndexableKind, classify_element_indexable};

    let interner = TypeInterner::new();

    // Member 1: plain object { a: number } — no index signature
    let member1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    // Member 2: object with string index { [s: string]: number }
    let member2 = interner.object_with_index(ObjectShape {
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

    // Create union: member1 | member2
    // Note: member1 is a structural subtype of member2 (every property is covered
    // by the string index signature). evaluate_type would collapse this union.
    let union_type = interner.union(vec![member1, member2]);

    // classify_element_indexable must preserve the Union variant so that
    // is_element_indexable can check each constituent independently.
    let kind = classify_element_indexable(&interner, union_type);
    match kind {
        ElementIndexableKind::Union(members) => {
            assert_eq!(members.len(), 2, "union should have 2 members");
        }
        other => {
            panic!(
                "expected ElementIndexableKind::Union, got {other:?}. \
                 Union was incorrectly collapsed by type evaluation."
            );
        }
    }
}

/// When the target has both string and number index signatures, an object with
/// only string-keyed properties (no numeric properties) should be assignable.
/// The number index is vacuously satisfied because the string index already
/// covers all keys and TypeScript requires `number_index_type <: string_index_type`.
///
/// Regression test for: `{ foo: fn } <: { [x: string]: T; [x: number]: T }`
/// failing with false TS2322/TS2345 when the source has no numeric properties.
#[test]
fn test_object_with_string_props_assignable_to_dual_index_target() {
    let interner = TypeInterner::new();

    // Source: { foo: string }
    let source = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name: interner.intern_string("foo"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    // Target: { [x: string]: string; [x: number]: string }
    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
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

    assert!(
        is_subtype_of(&interner, source, target),
        "{{ foo: string }} should be assignable to {{ [x: string]: string; [x: number]: string }}"
    );
}

/// Same as above but the source has NO properties at all (empty object).
/// Empty objects should also be assignable to dual index targets.
#[test]
fn test_empty_object_assignable_to_dual_index_target() {
    let interner = TypeInterner::new();

    let source = interner.object(vec![]);

    let target = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
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

    assert!(
        is_subtype_of(&interner, source, target),
        "Empty object should be assignable to {{ [x: string]: string; [x: number]: string }}"
    );
}
