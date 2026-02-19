//! Comprehensive tests for tuple type operations.
//!
//! These tests verify TypeScript's tuple type behavior:
//! - Tuple element access
//! - Tuple length
//! - Tuple spread/rest
//! - Tuple assignability

use super::*;
use crate::intern::TypeInterner;
use crate::subtype::SubtypeChecker;
use crate::types::{TupleElement, TypeData};

// =============================================================================
// Basic Tuple Construction Tests
// =============================================================================

#[test]
fn test_tuple_construction() {
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

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].type_id, TypeId::STRING);
        assert_eq!(elements[1].type_id, TypeId::NUMBER);
    } else {
        panic!("Expected tuple type");
    }
}

#[test]
fn test_empty_tuple() {
    let interner = TypeInterner::new();

    let empty_tuple = interner.tuple(vec![]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(empty_tuple) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 0);
    } else {
        panic!("Expected empty tuple type");
    }
}

#[test]
fn test_single_element_tuple() {
    let interner = TypeInterner::new();

    let single = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(single) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].type_id, TypeId::BOOLEAN);
    } else {
        panic!("Expected single-element tuple");
    }
}

// =============================================================================
// Tuple Subtype Tests
// =============================================================================

#[test]
fn test_tuple_same_type_is_subtype() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    assert!(
        checker.is_subtype_of(tuple, tuple),
        "Tuple should be subtype of itself"
    );
}

#[test]
fn test_tuple_element_subtype() {
    // [string, number] <: [string, any] because number <: any
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_1 = interner.tuple(vec![
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

    let tuple_2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    assert!(
        checker.is_subtype_of(tuple_1, tuple_2),
        "[string, number] should be subtype of [string, any]"
    );
}

#[test]
fn test_tuple_not_subtype_different_lengths() {
    // [string] is NOT a subtype of [string, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let shorter = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);

    let longer = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(shorter, longer),
        "Shorter tuple should not be subtype of longer tuple"
    );
}

#[test]
fn test_tuple_element_not_compatible() {
    // [string, number] is NOT a subtype of [number, number]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple_1 = interner.tuple(vec![
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

    let tuple_2 = interner.tuple(vec![
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

    assert!(
        !checker.is_subtype_of(tuple_1, tuple_2),
        "[string, number] should not be subtype of [number, number]"
    );
}

// =============================================================================
// Tuple with Rest Element Tests
// =============================================================================

#[test]
fn test_tuple_with_rest_element() {
    let interner = TypeInterner::new();

    let tuple_with_rest = interner.tuple(vec![
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
            rest: true,
        },
    ]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple_with_rest) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        assert!(!elements[0].rest);
        assert!(elements[1].rest);
    } else {
        panic!("Expected tuple type");
    }
}

// =============================================================================
// Tuple with Optional Element Tests
// =============================================================================

#[test]
fn test_tuple_with_optional_element() {
    let interner = TypeInterner::new();

    let tuple_with_optional = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple_with_optional) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        assert!(!elements[0].optional);
        assert!(elements[1].optional);
    } else {
        panic!("Expected tuple type");
    }
}

// =============================================================================
// Tuple vs Array Subtype Tests
// =============================================================================

#[test]
fn test_tuple_is_subtype_of_array() {
    // [string, number] <: (string | number)[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    let union_type = interner.union2(TypeId::STRING, TypeId::NUMBER);
    let array = interner.array(union_type);

    assert!(
        checker.is_subtype_of(tuple, array),
        "Tuple should be subtype of array of union"
    );
}

#[test]
fn test_tuple_is_subtype_of_string_array() {
    // [string, string] <: string[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
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

#[test]
fn test_tuple_not_subtype_of_incompatible_array() {
    // [string, string] is NOT a subtype of number[]
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let array = interner.array(TypeId::NUMBER);

    assert!(
        !checker.is_subtype_of(tuple, array),
        "[string, string] should not be subtype of number[]"
    );
}

// =============================================================================
// Named Tuple Elements Tests
// =============================================================================

#[test]
fn test_tuple_with_named_elements() {
    let interner = TypeInterner::new();

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(interner.intern_string("first")),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("second")),
            optional: false,
            rest: false,
        },
    ]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        assert_eq!(
            elements[0].name.map(|a| interner.resolve_atom(a)),
            Some("first".to_string())
        );
        assert_eq!(
            elements[1].name.map(|a| interner.resolve_atom(a)),
            Some("second".to_string())
        );
    } else {
        panic!("Expected tuple type");
    }
}

// =============================================================================
// Readonly Tuple Tests
// =============================================================================

#[test]
fn test_readonly_tuple_subtyping() {
    // T[] <: readonly T[] (covariance for readonly)
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

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

    let readonly_tuple = interner.readonly_type(tuple);

    // Same tuple should be subtype of its readonly version
    assert!(
        checker.is_subtype_of(tuple, readonly_tuple),
        "Tuple should be subtype of readonly Tuple"
    );
}

// =============================================================================
// Tuple Identity Tests
// =============================================================================

#[test]
fn test_tuple_identity_stability() {
    let interner = TypeInterner::new();

    let elements = vec![
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
    ];

    let tuple1 = interner.tuple(elements.clone());
    let tuple2 = interner.tuple(elements);

    assert_eq!(
        tuple1, tuple2,
        "Same tuple construction should produce same TypeId"
    );
}

// =============================================================================
// Tuple with Literal Types
// =============================================================================

#[test]
fn test_tuple_with_literal_types() {
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let literal_hello = interner.literal_string("hello");
    let literal_42 = interner.literal_number(42.0);

    let literal_tuple = interner.tuple(vec![
        TupleElement {
            type_id: literal_hello,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: literal_42,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let base_tuple = interner.tuple(vec![
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

    // ["hello", 42] <: [string, number]
    assert!(
        checker.is_subtype_of(literal_tuple, base_tuple),
        "Literal tuple should be subtype of base tuple"
    );
}

// =============================================================================
// Tuple with Union Elements
// =============================================================================

#[test]
fn test_tuple_with_union_element() {
    let interner = TypeInterner::new();

    let string_or_number = interner.union2(TypeId::STRING, TypeId::NUMBER);

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_or_number,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    if let Some(TypeData::Tuple(elements)) = interner.lookup(tuple) {
        let elements = interner.tuple_list(elements);
        assert_eq!(elements.len(), 2);
        // Second element should be a union
        if let Some(TypeData::Union(_)) = interner.lookup(elements[1].type_id) {
            // Good
        } else {
            panic!("Expected union type for second element");
        }
    } else {
        panic!("Expected tuple type");
    }
}

// =============================================================================
// Long Tuple Tests
// =============================================================================

#[test]
fn test_long_tuple() {
    let interner = TypeInterner::new();

    let elements: Vec<TupleElement> = (0..10)
        .map(|i| TupleElement {
            type_id: if i % 2 == 0 {
                TypeId::STRING
            } else {
                TypeId::NUMBER
            },
            name: None,
            optional: false,
            rest: false,
        })
        .collect();

    let long_tuple = interner.tuple(elements);

    if let Some(TypeData::Tuple(tuple_elements)) = interner.lookup(long_tuple) {
        let elements = interner.tuple_list(tuple_elements);
        assert_eq!(elements.len(), 10);
    } else {
        panic!("Expected long tuple");
    }
}
