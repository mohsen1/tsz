//! Comprehensive tests for tuple type operations.
//!
//! These tests verify TypeScript's tuple type behavior:
//! - Tuple element access
//! - Tuple length
//! - Tuple spread/rest
//! - Tuple assignability

use super::*;
use crate::intern::TypeInterner;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{TupleElement, TypeData, TypeParamInfo};

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

// =============================================================================
// Variadic Tuple Type Parameter Spread Assignability Tests
// =============================================================================

/// Concrete tuple [any, any] should NOT be assignable to [...T, ...P]
/// where T and P are type parameters constrained to any[].
/// TSC: "Source provides no match for variadic element at position 0 in target."
#[test]
fn test_concrete_tuple_not_assignable_to_double_type_param_spread() {
    let interner = TypeInterner::new();

    // Create type parameters: T extends any[], P extends any[]
    let any_array = interner.array(TypeId::ANY);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    let p_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("P"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // Source: [any, any]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::ANY,
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

    // Target: [...T, ...P]
    let target = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: p_param,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(source, target),
        "[any, any] should NOT be assignable to [...T, ...P] (type params)"
    );
}

/// Concrete tuple [any, any] should NOT be assignable to [...T]
/// where T is a type parameter constrained to any[].
#[test]
fn test_concrete_tuple_not_assignable_to_single_type_param_spread() {
    let interner = TypeInterner::new();

    let any_array = interner.array(TypeId::ANY);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(any_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // Source: [any, any]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::ANY,
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

    // Target: [...T]
    let target = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(source, target),
        "[any, any] should NOT be assignable to [...T] (type param)"
    );
}

/// Concrete tuple [any, any] SHOULD be assignable to [...any[]]
/// (concrete array spread, not a type parameter).
#[test]
fn test_concrete_tuple_assignable_to_concrete_array_spread() {
    let interner = TypeInterner::new();

    // Source: [any, any]
    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::ANY,
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

    // Target: [...any[]] — rest element with any[] (concrete array type)
    let any_array = interner.array(TypeId::ANY);
    let target = interner.tuple(vec![TupleElement {
        type_id: any_array,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(source, target),
        "[any, any] SHOULD be assignable to [...any[]] (concrete spread)"
    );
}

// =============================================================================
// Variadic Tuple Identity Tests: [...T] ↔ T
// =============================================================================

/// [...T] should be assignable to T (spread-unwrap identity).
/// In TSC, `[...T]` is structurally equivalent to `T` when T extends unknown[].
#[test]
fn test_spread_tuple_assignable_to_type_param() {
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // Source: [...T]
    let source = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(source, t_param),
        "[...T] should be assignable to T"
    );
}

/// T should be assignable to [...T] (wrap identity).
#[test]
fn test_type_param_assignable_to_spread_tuple() {
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // Target: [...T]
    let target = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(t_param, target),
        "T should be assignable to [...T]"
    );
}

/// T should be assignable to readonly [...T].
#[test]
fn test_type_param_assignable_to_readonly_spread_tuple() {
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    // Target: readonly [...T]
    let spread_tuple = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let readonly_target = interner.intern(TypeData::ReadonlyType(spread_tuple));

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(t_param, readonly_target),
        "T should be assignable to readonly [...T]"
    );
}

/// [...S] should be assignable to [...T] when S <: T (both single-rest type params).
#[test]
fn test_spread_tuple_subtype_preserves_type_param_relation() {
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    // S extends T (constraint is T itself)
    let s_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("S"),
        constraint: Some(t_param),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    let source = interner.tuple(vec![TupleElement {
        type_id: s_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let target = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        checker.is_subtype_of(source, target),
        "[...S] should be assignable to [...T] when S extends T"
    );
}

/// [...T] should NOT be assignable to [...U] when T and U are unrelated type params.
#[test]
fn test_spread_tuple_not_assignable_to_unrelated_spread() {
    let interner = TypeInterner::new();

    let unknown_array = interner.array(TypeId::UNKNOWN);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    let u_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));

    let source = interner.tuple(vec![TupleElement {
        type_id: t_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let target = interner.tuple(vec![TupleElement {
        type_id: u_param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let mut checker = SubtypeChecker::new(&interner);
    assert!(
        !checker.is_subtype_of(source, target),
        "[...T] should NOT be assignable to [...U] when T and U are unrelated"
    );
}
