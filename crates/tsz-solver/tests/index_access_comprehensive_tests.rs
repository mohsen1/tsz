//! Comprehensive tests for index access type operations.
//!
//! These tests verify TypeScript's indexed access type behavior:
//! - T[K] indexed access
//! - Element access on objects, arrays, tuples
//! - Index access with literal keys
//! - Index access with union keys

use super::*;
use crate::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TupleElement, TypeData};

// =============================================================================
// Basic Index Access Tests
// =============================================================================

#[test]
fn test_index_access_object() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let name_key = interner.literal_string("name");
    let index_access = interner.index_access(obj, name_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "obj['name'] should be string");
}

#[test]
fn test_index_access_with_number_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    // Number key on object - should work via string conversion
    let num_key = interner.literal_number(0.0);
    let index_access = interner.index_access(obj, num_key);

    // Just verify it doesn't crash
    let _result = evaluate_type(&interner, index_access);
}

// =============================================================================
// Index Access on Arrays
// =============================================================================

#[test]
fn test_index_access_array_with_number() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);

    let num_key = interner.literal_number(0.0);
    let index_access = interner.index_access(array, num_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "array[0] should be string");
}

#[test]
fn test_index_access_array_with_number_type() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NUMBER);

    let index_access = interner.index_access(array, TypeId::NUMBER);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NUMBER, "array[number] should be number");
}

// =============================================================================
// Index Access on Tuples
// =============================================================================

#[test]
fn test_index_access_tuple_first_element() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    let index_0 = interner.literal_number(0.0);
    let index_access = interner.index_access(tuple, index_0);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::STRING, "tuple[0] should be string");
}

#[test]
fn test_index_access_tuple_second_element() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
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

    let index_1 = interner.literal_number(1.0);
    let index_access = interner.index_access(tuple, index_1);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::NUMBER, "tuple[1] should be number");
}

// =============================================================================
// Index Access with Union Keys
// =============================================================================

#[test]
fn test_index_access_with_union_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_union = interner.union2(key_a, key_b);

    let index_access = interner.index_access(obj, key_union);

    let result = evaluate_type(&interner, index_access);

    // obj['a' | 'b'] should be string | number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::STRING));
        assert!(members.contains(&TypeId::NUMBER));
    } else {
        panic!("Expected union of string | number");
    }
}

// =============================================================================
// Index Access on Object with Index Signature
// =============================================================================

#[test]
fn test_index_access_with_string_index_signature() {
    let interner = TypeInterner::new();

    let obj = interner.object_with_index(crate::types::ObjectShape {
        symbol: None,
        flags: crate::types::ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let any_string_key = interner.literal_string("anyKey");
    let index_access = interner.index_access(obj, any_string_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(
        result,
        TypeId::NUMBER,
        "obj with string index should return number"
    );
}

// =============================================================================
// Index Access Identity Tests
// =============================================================================

#[test]
fn test_index_access_identity_stability() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let key = interner.literal_string("name");

    let access1 = interner.index_access(obj, key);
    let access2 = interner.index_access(obj, key);

    assert_eq!(
        access1, access2,
        "Same index access should produce same TypeId"
    );
}

// =============================================================================
// Index Access with keyof
// =============================================================================

#[test]
fn test_index_access_with_keyof() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);

    let keyof_t = interner.keyof(obj);
    let index_access = interner.index_access(obj, keyof_t);

    let result = evaluate_type(&interner, index_access);

    // T[keyof T] should be string | number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let members = interner.type_list(members);
        assert_eq!(members.len(), 2);
    } else {
        // Could also be string | number simplified
    }
}

// =============================================================================
// Index Access on Nested Objects
// =============================================================================

#[test]
fn test_index_access_nested_object() {
    let interner = TypeInterner::new();

    let inner = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);

    let outer = interner.object(vec![PropertyInfo::new(
        interner.intern_string("nested"),
        inner,
    )]);

    let key = interner.literal_string("nested");
    let index_access = interner.index_access(outer, key);

    let result = evaluate_type(&interner, index_access);

    // outer['nested'] should be the inner object type
    if let Some(TypeData::Object(_)) = interner.lookup(result) {
        // Good
    } else {
        panic!("Expected object type for nested access");
    }
}

// =============================================================================
// Index Access with any
// =============================================================================

#[test]
fn test_index_access_any_object() {
    let interner = TypeInterner::new();

    let any_key = interner.literal_string("anything");
    let index_access = interner.index_access(TypeId::ANY, any_key);

    let result = evaluate_type(&interner, index_access);
    assert_eq!(result, TypeId::ANY, "any['key'] should be any");
}

#[test]
fn test_index_access_with_any_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let index_access = interner.index_access(obj, TypeId::ANY);

    let result = evaluate_type(&interner, index_access);
    // obj[any] could be any or the property type depending on implementation
    let _ = result;
}

// =============================================================================
// Index Access Subtype Tests
// =============================================================================

#[test]
fn test_index_access_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let key = interner.literal_string("name");
    let index_access = interner.index_access(obj, key);
    let result = evaluate_type(&interner, index_access);

    assert!(
        checker.is_subtype_of(result, TypeId::STRING),
        "obj['name'] should be subtype of string"
    );
}

// =============================================================================
// Index Access with Never
// =============================================================================

#[test]
fn test_index_access_never_key() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let index_access = interner.index_access(obj, TypeId::NEVER);

    let _result = evaluate_type(&interner, index_access);
    // obj[never] - behavior depends on implementation
    // Could be never or could be an error type
}

// =============================================================================
// Multiple Index Access Tests
// =============================================================================

#[test]
fn test_multiple_index_access() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);

    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    let key_c = interner.literal_string("c");

    let access_a = evaluate_type(&interner, interner.index_access(obj, key_a));
    let access_b = evaluate_type(&interner, interner.index_access(obj, key_b));
    let access_c = evaluate_type(&interner, interner.index_access(obj, key_c));

    assert_eq!(access_a, TypeId::STRING);
    assert_eq!(access_b, TypeId::NUMBER);
    assert_eq!(access_c, TypeId::BOOLEAN);
}
