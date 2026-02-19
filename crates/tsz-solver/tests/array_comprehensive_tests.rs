//! Comprehensive tests for array type operations.
//!
//! These tests verify TypeScript's array type behavior:
//! - Array construction
//! - Array subtype relationships
//! - Array element access
//! - Readonly arrays

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{PropertyInfo, TypeData};

// =============================================================================
// Basic Array Construction Tests
// =============================================================================

#[test]
fn test_array_construction() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::STRING);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_array_of_number() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NUMBER);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::NUMBER);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_array_of_boolean() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::BOOLEAN);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::BOOLEAN);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_array_of_any() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::ANY);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::ANY);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_array_of_never() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NEVER);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::NEVER);
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn test_array_of_union() {
    let interner = TypeInterner::new();

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let array = interner.array(string_or_number);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        // Element should be the union type
        if let Some(TypeData::Union(_)) = interner.lookup(element) {
            // Good
        } else {
            panic!("Expected union element type");
        }
    } else {
        panic!("Expected array type");
    }
}

// =============================================================================
// Array Subtype Tests
// =============================================================================

#[test]
fn test_array_same_type_is_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let array = interner.array(TypeId::STRING);

    assert!(
        checker.is_subtype_of(array, array),
        "Array should be subtype of itself"
    );
}

#[test]
fn test_array_element_covariance() {
    // string[] <: (string | number)[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let union_array = interner.array(string_or_number);

    assert!(
        checker.is_subtype_of(string_array, union_array),
        "string[] should be subtype of (string | number)[]"
    );
}

#[test]
fn test_array_not_subtype_incompatible_element() {
    // string[] is NOT <: number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    assert!(
        !checker.is_subtype_of(string_array, number_array),
        "string[] should not be subtype of number[]"
    );
}

// =============================================================================
// Array vs Tuple Subtype Tests
// =============================================================================

#[test]
fn test_tuple_is_subtype_of_array() {
    // [string, string] <: string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let array = interner.array(TypeId::STRING);

    assert!(
        checker.is_subtype_of(tuple, array),
        "[string, string] should be subtype of string[]"
    );
}

// =============================================================================
// Readonly Array Tests
// =============================================================================

#[test]
fn test_readonly_array_construction() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.readonly_type(array);

    if let Some(TypeData::ReadonlyType(inner)) = interner.lookup(readonly_array) {
        assert_eq!(inner, array);
    } else {
        panic!("Expected readonly type");
    }
}

#[test]
fn test_array_subtype_of_readonly_array() {
    // T[] <: readonly T[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.readonly_type(array);

    assert!(
        checker.is_subtype_of(array, readonly_array),
        "T[] should be subtype of readonly T[]"
    );
}

// =============================================================================
// Array with any
// =============================================================================

#[test]
fn test_array_assignable_to_any() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let array = interner.array(TypeId::STRING);

    assert!(
        checker.is_subtype_of(array, TypeId::ANY),
        "Array should be subtype of any"
    );
}

#[test]
fn test_any_assignable_to_array() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let array = interner.array(TypeId::STRING);

    assert!(
        checker.is_subtype_of(TypeId::ANY, array),
        "any should be subtype of array"
    );
}

// =============================================================================
// Array with never
// =============================================================================

#[test]
fn test_never_assignable_to_array() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let array = interner.array(TypeId::STRING);

    assert!(
        checker.is_subtype_of(TypeId::NEVER, array),
        "never should be subtype of array"
    );
}

#[test]
fn test_array_of_never_is_never_array() {
    let interner = TypeInterner::new();

    let array = interner.array(TypeId::NEVER);

    // never[] is a valid type, though rarely useful
    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        assert_eq!(element, TypeId::NEVER);
    } else {
        panic!("Expected array type");
    }
}

// =============================================================================
// Array Identity Tests
// =============================================================================

#[test]
fn test_array_identity_stability() {
    let interner = TypeInterner::new();

    let array1 = interner.array(TypeId::STRING);
    let array2 = interner.array(TypeId::STRING);

    assert_eq!(
        array1, array2,
        "Same array construction should produce same TypeId"
    );
}

// =============================================================================
// Nested Array Tests
// =============================================================================

#[test]
fn test_nested_array() {
    let interner = TypeInterner::new();

    let inner_array = interner.array(TypeId::NUMBER);
    let outer_array = interner.array(inner_array);

    if let Some(TypeData::Array(inner)) = interner.lookup(outer_array) {
        if let Some(TypeData::Array(_)) = interner.lookup(inner) {
            // Good - nested array
        } else {
            panic!("Expected inner array type");
        }
    } else {
        panic!("Expected outer array type");
    }
}

#[test]
fn test_deeply_nested_array() {
    let interner = TypeInterner::new();

    let level1 = interner.array(TypeId::NUMBER);
    let level2 = interner.array(level1);
    let level3 = interner.array(level2);

    if let Some(TypeData::Array(_)) = interner.lookup(level3) {
        // Good
    } else {
        panic!("Expected array type");
    }
}

// =============================================================================
// Array of Object Types
// =============================================================================

#[test]
fn test_array_of_objects() {
    let interner = TypeInterner::new();

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let array = interner.array(obj);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        if let Some(TypeData::Object(_)) = interner.lookup(element) {
            // Good
        } else {
            panic!("Expected object element type");
        }
    } else {
        panic!("Expected array type");
    }
}

// =============================================================================
// Array Assignability with Objects
// =============================================================================

#[test]
fn test_array_of_objects_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let obj_extended = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("age"), TypeId::NUMBER),
    ]);

    let obj_base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);

    let extended_array = interner.array(obj_extended);
    let base_array = interner.array(obj_base);

    // {name, age}[] <: {name}[] (covariance)
    assert!(
        checker.is_subtype_of(extended_array, base_array),
        "Array of extended objects should be subtype of array of base objects"
    );
}

// =============================================================================
// Array with Function Elements
// =============================================================================

#[test]
fn test_array_of_functions() {
    let interner = TypeInterner::new();

    let func = interner.function(crate::types::FunctionShape {
        params: vec![crate::types::ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let array = interner.array(func);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        if let Some(TypeData::Function(_)) = interner.lookup(element) {
            // Good
        } else {
            panic!("Expected function element type");
        }
    } else {
        panic!("Expected array type");
    }
}

// =============================================================================
// Array with Literal Types
// =============================================================================

#[test]
fn test_array_of_string_literals() {
    let interner = TypeInterner::new();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let literal_union = interner.union2(hello, world);

    let array = interner.array(literal_union);

    if let Some(TypeData::Array(element)) = interner.lookup(array) {
        if let Some(TypeData::Union(_)) = interner.lookup(element) {
            // Good
        } else {
            panic!("Expected union element type");
        }
    } else {
        panic!("Expected array type");
    }
}
