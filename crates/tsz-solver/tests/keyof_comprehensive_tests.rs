//! Comprehensive tests for keyof operator evaluation.
//!
//! These tests verify TypeScript's keyof operator behavior:
//! - keyof T extracts the keys of type T
//! - keyof (A | B) = keyof A & keyof B (distributive contravariance)
//! - keyof (A & B) = keyof A | keyof B (covariance)

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, IndexSignature, ObjectFlags, ObjectShape,
    ParamInfo, TypeData,
};

/// Helper to check if a type is a union containing specific literals
fn union_contains_literals(interner: &TypeInterner, type_id: TypeId, expected: &[&str]) -> bool {
    let Some(TypeData::Union(members)) = interner.lookup(type_id) else {
        return false;
    };
    let member_list = interner.type_list(members);
    let expected_set: std::collections::HashSet<String> =
        expected.iter().map(|s| s.to_string()).collect();

    let mut found_literals = std::collections::HashSet::new();
    for &member in member_list.iter() {
        if let Some(TypeData::Literal(crate::types::LiteralValue::String(atom))) =
            interner.lookup(member)
        {
            found_literals.insert(interner.resolve_atom(atom));
        }
    }

    found_literals == expected_set
}

// =============================================================================
// Basic keyof on Object Types
// =============================================================================

#[test]
fn test_keyof_empty_object_is_never() {
    let interner = TypeInterner::new();
    let empty_obj = interner.object(vec![]);
    let keyof_empty = interner.keyof(empty_obj);
    let result = evaluate_type(&interner, keyof_empty);
    assert_eq!(result, TypeId::NEVER, "keyof {{}} should be never");
}

#[test]
fn test_keyof_single_property() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);
    let keyof_obj = interner.keyof(obj);
    let result = evaluate_type(&interner, keyof_obj);

    // keyof {name: string} should be "name"
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(_))) = interner.lookup(result)
    {
        // Good - single property becomes a literal
    } else {
        panic!(
            "Expected string literal 'name', got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_keyof_multiple_properties() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);
    let keyof_obj = interner.keyof(obj);
    let result = evaluate_type(&interner, keyof_obj);

    assert!(
        union_contains_literals(&interner, result, &["a", "b", "c"]),
        "keyof should return union of property names"
    );
}

#[test]
fn test_keyof_object_with_optional_property() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("required"), TypeId::STRING),
        PropertyInfo::opt(interner.intern_string("optional"), TypeId::NUMBER),
    ]);
    let keyof_obj = interner.keyof(obj);
    let result = evaluate_type(&interner, keyof_obj);

    // Optional properties are still in keyof
    assert!(
        union_contains_literals(&interner, result, &["required", "optional"]),
        "keyof should include optional properties"
    );
}

// =============================================================================
// keyof on Intrinsic Types
// =============================================================================

#[test]
fn test_keyof_any_is_string_number_symbol() {
    let interner = TypeInterner::new();
    let keyof_any = interner.keyof(TypeId::ANY);
    let result = evaluate_type(&interner, keyof_any);

    // keyof any = string | number | symbol
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert_eq!(member_list.len(), 3);
        assert!(member_list.contains(&TypeId::STRING));
        assert!(member_list.contains(&TypeId::NUMBER));
        assert!(member_list.contains(&TypeId::SYMBOL));
    } else {
        panic!("Expected union of string | number | symbol");
    }
}

#[test]
fn test_keyof_unknown_is_never() {
    let interner = TypeInterner::new();
    let keyof_unknown = interner.keyof(TypeId::UNKNOWN);
    let result = evaluate_type(&interner, keyof_unknown);
    assert_eq!(result, TypeId::NEVER, "keyof unknown should be never");
}

#[test]
fn test_keyof_never_is_property_key() {
    let interner = TypeInterner::new();
    let keyof_never = interner.keyof(TypeId::NEVER);
    let result = evaluate_type(&interner, keyof_never);
    let property_key = interner.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL);
    assert_eq!(
        result, property_key,
        "keyof never should be string | number | symbol"
    );
}

#[test]
fn test_keyof_void_is_never() {
    let interner = TypeInterner::new();
    let keyof_void = interner.keyof(TypeId::VOID);
    let result = evaluate_type(&interner, keyof_void);
    assert_eq!(result, TypeId::NEVER, "keyof void should be never");
}

#[test]
fn test_keyof_null_is_never() {
    let interner = TypeInterner::new();
    let keyof_null = interner.keyof(TypeId::NULL);
    let result = evaluate_type(&interner, keyof_null);
    assert_eq!(result, TypeId::NEVER, "keyof null should be never");
}

#[test]
fn test_keyof_undefined_is_never() {
    let interner = TypeInterner::new();
    let keyof_undefined = interner.keyof(TypeId::UNDEFINED);
    let result = evaluate_type(&interner, keyof_undefined);
    assert_eq!(result, TypeId::NEVER, "keyof undefined should be never");
}

#[test]
fn test_keyof_string_is_apparent_members() {
    let interner = TypeInterner::new();
    let keyof_string = interner.keyof(TypeId::STRING);
    let result = evaluate_type(&interner, keyof_string);

    // keyof string should include apparent members like "length", "charAt", etc.
    // This is the apparent type of string primitives
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        // Should have many string methods
        assert!(
            member_list.len() > 10,
            "keyof string should have many apparent members"
        );
    } else {
        // Could also be string if it's simplified
        // The key point is it shouldn't be never
        assert_ne!(result, TypeId::NEVER, "keyof string should not be never");
    }
}

// =============================================================================
// keyof on Union Types (Contravariance)
// =============================================================================

#[test]
fn test_keyof_union_is_intersection() {
    // keyof ({a: string} | {b: number}) = keyof {a: string} & keyof {b: number} = never
    // (no common keys)
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let union_ab = interner.union2(obj_a, obj_b);

    let keyof_union = interner.keyof(union_ab);
    let result = evaluate_type(&interner, keyof_union);

    assert_eq!(
        result,
        TypeId::NEVER,
        "keyof ({{a}} | {{b}}) should be never (no common keys)"
    );
}

#[test]
fn test_keyof_union_with_common_key() {
    // keyof ({a: string, b: number} | {a: number, c: boolean}) = "a"
    // (only 'a' is common)
    let interner = TypeInterner::new();

    let obj_1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let obj_2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);
    let union_12 = interner.union2(obj_1, obj_2);

    let keyof_union = interner.keyof(union_12);
    let result = evaluate_type(&interner, keyof_union);

    // Result should be just "a"
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(_))) = interner.lookup(result)
    {
        // Good
    } else {
        panic!(
            "Expected string literal 'a', got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_keyof_union_with_all_common_keys() {
    // keyof ({a: string, b: number} | {a: number, b: string}) = "a" | "b"
    let interner = TypeInterner::new();

    let obj_1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let obj_2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    let union_12 = interner.union2(obj_1, obj_2);

    let keyof_union = interner.keyof(union_12);
    let result = evaluate_type(&interner, keyof_union);

    assert!(
        union_contains_literals(&interner, result, &["a", "b"]),
        "keyof union with all common keys should return union of those keys"
    );
}

// =============================================================================
// keyof on Intersection Types (Covariance)
// =============================================================================

#[test]
fn test_keyof_intersection_is_union() {
    // keyof ({a: string} & {b: number}) = keyof {a: string} | keyof {b: number} = "a" | "b"
    let interner = TypeInterner::new();

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);
    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);
    let intersection_ab = interner.intersection2(obj_a, obj_b);

    let keyof_intersection = interner.keyof(intersection_ab);
    let result = evaluate_type(&interner, keyof_intersection);

    assert!(
        union_contains_literals(&interner, result, &["a", "b"]),
        "keyof ({{a}} & {{b}}) should be 'a' | 'b'"
    );
}

#[test]
fn test_keyof_intersection_with_overlapping_keys() {
    // keyof ({a: string, b: number} & {b: string, c: boolean}) = "a" | "b" | "c"
    let interner = TypeInterner::new();

    let obj_1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let obj_2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("c"), TypeId::BOOLEAN),
    ]);
    let intersection_12 = interner.intersection2(obj_1, obj_2);

    let keyof_intersection = interner.keyof(intersection_12);
    let result = evaluate_type(&interner, keyof_intersection);

    assert!(
        union_contains_literals(&interner, result, &["a", "b", "c"]),
        "keyof intersection should return union of all keys"
    );
}

// =============================================================================
// keyof on Array and Tuple Types
// =============================================================================

#[test]
fn test_keyof_array_includes_number() {
    let interner = TypeInterner::new();
    let arr = interner.array(TypeId::STRING);
    let keyof_arr = interner.keyof(arr);
    let result = evaluate_type(&interner, keyof_arr);

    // keyof Array<T> should include number (for index access)
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert!(
            member_list.contains(&TypeId::NUMBER),
            "keyof array should include number"
        );
    } else {
        panic!(
            "Expected union for keyof array, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_keyof_tuple_includes_numeric_indices() {
    let interner = TypeInterner::new();
    let tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let keyof_tuple = interner.keyof(tuple);
    let result = evaluate_type(&interner, keyof_tuple);

    // keyof [string, number] should include "0", "1", and number (for array methods)
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        // Should have numeric indices and number
        assert!(
            member_list.contains(&TypeId::NUMBER),
            "keyof tuple should include number"
        );
    } else {
        panic!("Expected union for keyof tuple");
    }
}

// =============================================================================
// keyof on Object with Index Signatures
// =============================================================================

#[test]
fn test_keyof_object_with_string_index_includes_string_and_number() {
    let interner = TypeInterner::new();
    let obj_with_index = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::new(
            interner.intern_string("fixed"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol_index: None,
    });

    let keyof_obj = interner.keyof(obj_with_index);
    let result = evaluate_type(&interner, keyof_obj);

    // keyof { [x: string]: number, fixed: string } should include string, number, and "fixed"
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert!(
            member_list.contains(&TypeId::STRING),
            "keyof should include string from string index"
        );
        assert!(
            member_list.contains(&TypeId::NUMBER),
            "keyof should include number (JS arrays allow numeric access)"
        );
    } else {
        panic!("Expected union for keyof object with string index");
    }
}

// Regression: `keyof` over a shape that carries both a string-keyed and a
// symbol-keyed index signature must include `string | number | symbol`, not
// drop the symbol bit because both signatures landed in the same `string_index`
// slot. See tsz issue #9772.
#[test]
fn test_keyof_mixed_string_and_symbol_index_signatures_includes_symbol() {
    let interner = TypeInterner::new();
    let obj = interner.object_with_index(ObjectShape {
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
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let result = evaluate_type(&interner, interner.keyof(obj));
    let Some(TypeData::Union(members)) = interner.lookup(result) else {
        panic!("expected union for keyof of mixed string+symbol index");
    };
    let member_list = interner.type_list(members);
    assert!(
        member_list.contains(&TypeId::STRING),
        "keyof should include string"
    );
    assert!(
        member_list.contains(&TypeId::NUMBER),
        "keyof should include number (string indexes are also numeric-key compatible)"
    );
    assert!(
        member_list.contains(&TypeId::SYMBOL),
        "keyof must include symbol when the shape carries a [k: symbol]: V signature"
    );
}

// Regression: a renamed iteration variable must not change the answer — proves
// the fix is structural, not keyed on identifier spelling.
#[test]
fn test_keyof_mixed_string_and_symbol_index_signatures_renamed_params() {
    let interner = TypeInterner::new();
    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: Some(interner.intern_string("anyName")),
        }),
        number_index: None,
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: Some(interner.intern_string("anotherName")),
        }),
    });
    let result = evaluate_type(&interner, interner.keyof(obj));
    let Some(TypeData::Union(members)) = interner.lookup(result) else {
        panic!("expected union for keyof of mixed string+symbol index");
    };
    let member_list = interner.type_list(members);
    assert!(member_list.contains(&TypeId::STRING));
    assert!(member_list.contains(&TypeId::NUMBER));
    assert!(member_list.contains(&TypeId::SYMBOL));
}

// Regression: when both indexes carry distinct value types, neither side's
// value type should leak through `keyof`. Only the key shape changes.
#[test]
fn test_keyof_mixed_indexes_preserves_value_type_independence() {
    let interner = TypeInterner::new();
    let obj = interner.object_with_index(ObjectShape {
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
        symbol_index: Some(IndexSignature {
            key_type: TypeId::SYMBOL,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });
    // Indexed access through symbol must yield the symbol-index value type
    // (STRING), not the unioned `number | string` produced by smudging both
    // signatures into one slot.
    let by_symbol = evaluate_type(&interner, interner.index_access(obj, TypeId::SYMBOL));
    assert_eq!(
        by_symbol,
        TypeId::STRING,
        "obj[symbol] should be the symbol-index value type (string), got {by_symbol:?}"
    );

    // Indexed access through string must yield the string-index value type.
    let by_string = evaluate_type(&interner, interner.index_access(obj, TypeId::STRING));
    assert_eq!(
        by_string,
        TypeId::NUMBER,
        "obj[string] should be the string-index value type (number), got {by_string:?}"
    );
}

#[test]
fn test_keyof_object_with_number_index_includes_number() {
    let interner = TypeInterner::new();
    let obj_with_index = interner.object_with_index(ObjectShape {
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
        symbol_index: None,
    });

    let keyof_obj = interner.keyof(obj_with_index);
    let result = evaluate_type(&interner, keyof_obj);

    // keyof { [x: number]: string } should include number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert!(
            member_list.contains(&TypeId::NUMBER),
            "keyof should include number from number index"
        );
    } else if result == TypeId::NUMBER {
        // Could be simplified to just number
    } else {
        panic!("Expected number or union containing number");
    }
}

// =============================================================================
// keyof on Readonly Types
// =============================================================================

#[test]
fn test_keyof_readonly_same_as_keyof_inner() {
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let readonly_obj = interner.readonly_type(obj);

    let keyof_readonly = interner.keyof(readonly_obj);
    let result = evaluate_type(&interner, keyof_readonly);

    assert!(
        union_contains_literals(&interner, result, &["a", "b"]),
        "keyof Readonly<T> should be same as keyof T"
    );
}

// =============================================================================
// keyof Identity Tests
// =============================================================================

#[test]
fn test_keyof_produces_stable_result() {
    // Same object type should produce same keyof result
    let interner = TypeInterner::new();
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let keyof1 = interner.keyof(obj);
    let keyof2 = interner.keyof(obj);

    assert_eq!(keyof1, keyof2, "keyof should produce stable results");

    let result1 = evaluate_type(&interner, keyof1);
    let result2 = evaluate_type(&interner, keyof2);

    assert_eq!(
        result1, result2,
        "evaluated keyof should produce stable results"
    );
}

#[test]
fn test_keyof_property_order_independence() {
    // Property order shouldn't affect keyof result
    let interner = TypeInterner::new();

    let obj1 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
    ]);
    let obj2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("b"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("a"), TypeId::STRING),
    ]);

    let keyof1 = interner.keyof(obj1);
    let keyof2 = interner.keyof(obj2);

    let result1 = evaluate_type(&interner, keyof1);
    let result2 = evaluate_type(&interner, keyof2);

    assert_eq!(
        result1, result2,
        "keyof should be independent of property order"
    );
}

// =============================================================================
// keyof with Nested Types
// =============================================================================

#[test]
fn test_keyof_nested_object() {
    // keyof { outer: { inner: string } } = "outer"
    // (only the top-level keys)
    let interner = TypeInterner::new();

    let inner_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::STRING,
    )]);
    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("outer"),
        inner_obj,
    )]);

    let keyof_outer = interner.keyof(outer_obj);
    let result = evaluate_type(&interner, keyof_outer);

    // Should be just "outer", not "outer" | "inner"
    if let Some(TypeData::Literal(crate::types::LiteralValue::String(_))) = interner.lookup(result)
    {
        // Good - only top-level key
    } else {
        panic!(
            "Expected single literal 'outer', got {:?}",
            interner.lookup(result)
        );
    }
}

// =============================================================================
// keyof on Callable Types (static index signatures on class constructors)
// =============================================================================

#[test]
fn test_keyof_callable_with_string_index_includes_string_and_number() {
    let interner = TypeInterner::new();
    // Simulates: class B { static [s: string]: number; }
    // typeof B is a Callable with string index signature
    let callable = interner.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let keyof_callable = interner.keyof(callable);
    let result = evaluate_type(&interner, keyof_callable);

    // keyof typeof B should include string and number
    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert!(
            member_list.contains(&TypeId::STRING),
            "keyof callable with string index should include string"
        );
        assert!(
            member_list.contains(&TypeId::NUMBER),
            "keyof callable with string index should include number"
        );
    } else {
        panic!(
            "Expected union for keyof callable with string index, got {:?}",
            interner.lookup(result)
        );
    }
}

#[test]
fn test_keyof_callable_with_number_index_includes_number() {
    let interner = TypeInterner::new();
    // Simulates: class B { static [s: number]: 42 | 233; }
    let num_value = interner.union2(
        interner.literal_number(42.0),
        interner.literal_number(233.0),
    );
    let callable = interner.callable(CallableShape {
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: num_value,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let keyof_callable = interner.keyof(callable);
    let result = evaluate_type(&interner, keyof_callable);

    assert_eq!(
        result,
        TypeId::NUMBER,
        "keyof callable with only number index should be number"
    );
}

#[test]
fn test_keyof_callable_with_properties_and_index() {
    let interner = TypeInterner::new();
    // Simulates: class B { static x: string; static [s: string]: number; }
    let callable = interner.callable(CallableShape {
        properties: vec![PropertyInfo::new(
            interner.intern_string("x"),
            TypeId::STRING,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let keyof_callable = interner.keyof(callable);
    let result = evaluate_type(&interner, keyof_callable);

    if let Some(TypeData::Union(members)) = interner.lookup(result) {
        let member_list = interner.type_list(members);
        assert!(
            member_list.contains(&TypeId::STRING),
            "keyof should include string from string index"
        );
    } else {
        panic!("Expected union, got {:?}", interner.lookup(result));
    }
}

// =============================================================================
// Index Access on Callable Types (type-level (typeof B)["foo"])
// =============================================================================

#[test]
fn test_index_access_callable_string_literal_via_string_index() {
    let interner = TypeInterner::new();
    // Simulates: (typeof B)["foo"] where B has static [s: string]: number
    let callable = interner.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let index_access = interner.index_access(callable, interner.literal_string("foo"));
    let result = evaluate_type(&interner, index_access);

    assert_eq!(
        result,
        TypeId::NUMBER,
        "(typeof B)[\"foo\"] should resolve to number via string index"
    );
}

#[test]
fn test_index_access_callable_number_literal_via_number_index() {
    let interner = TypeInterner::new();
    // Simulates: (typeof B)[42] where B has static [s: number]: 42 | 233
    let num_value = interner.union2(
        interner.literal_number(42.0),
        interner.literal_number(233.0),
    );
    let callable = interner.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: num_value,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let index_access = interner.index_access(callable, interner.literal_number(42.0));
    let result = evaluate_type(&interner, index_access);

    assert_eq!(
        result, num_value,
        "(typeof B)[42] should resolve to 42 | 233 via number index"
    );
}

#[test]
fn test_index_access_callable_string_intrinsic() {
    let interner = TypeInterner::new();
    // Simulates: (typeof B)[string] where B has static [s: string]: number
    let callable = interner.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let index_access = interner.index_access(callable, TypeId::STRING);
    let result = evaluate_type(&interner, index_access);

    assert_eq!(
        result,
        TypeId::NUMBER,
        "(typeof B)[string] should resolve to number"
    );
}

#[test]
fn test_index_access_callable_number_intrinsic() {
    let interner = TypeInterner::new();
    // Simulates: (typeof B)[number] where B has static [s: number]: 42 | 233
    let num_value = interner.union2(
        interner.literal_number(42.0),
        interner.literal_number(233.0),
    );
    let callable = interner.callable(CallableShape {
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: num_value,
            readonly: true,
            param_name: None,
        }),
        ..CallableShape::default()
    });

    let index_access = interner.index_access(callable, TypeId::NUMBER);
    let result = evaluate_type(&interner, index_access);

    assert_eq!(
        result, num_value,
        "(typeof B)[number] should resolve to 42 | 233 via number index"
    );
}

// =============================================================================
// keyof on Bare Function / Constructor Types (issue #9721)
//
// `keyof` of a type whose only structure is signature(s) and no own properties
// or index signatures is `never` in tsc.  Bare function types (`() => T`) lower
// to `TypeData::Function`; bare constructor types (`new () => T`) lower to a
// `TypeData::Callable` with only construct signatures.  Both must collapse to
// `never` so `[K] extends [never]` / `Equal<K, never>` matches tsc.
// =============================================================================

#[test]
fn test_keyof_bare_function_type_is_never() {
    let interner = TypeInterner::new();
    // Simulates: type Fn = () => void
    let func = interner.function(FunctionShape::new(Vec::new(), TypeId::VOID));

    let result = evaluate_type(&interner, interner.keyof(func));
    assert_eq!(
        result,
        TypeId::NEVER,
        "keyof (() => void) should be never, got {:?}",
        interner.lookup(result)
    );
}

#[test]
fn test_keyof_function_with_params_is_never() {
    let interner = TypeInterner::new();
    // Simulates: type Fn = (x: number, y: string) => boolean
    let func = interner.function(FunctionShape::new(
        vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
        ],
        TypeId::BOOLEAN,
    ));

    let result = evaluate_type(&interner, interner.keyof(func));
    assert_eq!(
        result,
        TypeId::NEVER,
        "keyof a function with params should still be never"
    );
}

#[test]
fn test_keyof_constructor_only_callable_is_never() {
    // Positive control from the issue's boundary table: `new () => object`
    // already collapses to never via the Callable arm; pin it down so a future
    // refactor of either path keeps the two behaviors symmetric.
    let interner = TypeInterner::new();
    let ctor = interner.callable(CallableShape {
        construct_signatures: vec![CallSignature::new(Vec::new(), TypeId::OBJECT)],
        ..CallableShape::default()
    });

    let result = evaluate_type(&interner, interner.keyof(ctor));
    assert_eq!(
        result,
        TypeId::NEVER,
        "keyof (new () => object) should be never"
    );
}

#[test]
fn test_keyof_call_only_callable_is_never() {
    // A `Callable` with one call signature and no properties / indices arises
    // for overloaded function types in a type literal.  Same key space as a
    // bare function type, so keyof must be never.
    let interner = TypeInterner::new();
    let call_only = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(Vec::new(), TypeId::VOID)],
        ..CallableShape::default()
    });

    let result = evaluate_type(&interner, interner.keyof(call_only));
    assert_eq!(
        result,
        TypeId::NEVER,
        "keyof a call-signature-only Callable should be never"
    );
}

#[test]
fn test_keyof_callable_with_properties_unchanged_by_function_fix() {
    // Negative control: a callable that *does* have properties still returns
    // the property keys (the fix must not over-reach into Callable).
    let interner = TypeInterner::new();
    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature::new(Vec::new(), TypeId::VOID)],
        properties: vec![PropertyInfo::new(
            interner.intern_string("prop"),
            TypeId::NUMBER,
        )],
        ..CallableShape::default()
    });

    let result = evaluate_type(&interner, interner.keyof(callable));
    let expected_prop = interner.literal_string("prop");
    assert_eq!(
        result, expected_prop,
        "keyof {{ (): void; prop: number }} should be \"prop\""
    );
}

#[test]
fn test_keyof_function_extends_never_is_true_branch() {
    use crate::types::{ConditionalType, TupleElement};
    // The whole point of the fix: the conditional `[keyof Fn] extends [never]`
    // must take the true branch.  Build the conditional in the solver, evaluate
    // it, and confirm we land on the true branch (modelled here by `TypeId::STRING`).
    let interner = TypeInterner::new();
    let func = interner.function(FunctionShape::new(Vec::new(), TypeId::VOID));
    let keyof_fn = interner.keyof(func);

    let wrap = |id| TupleElement {
        type_id: id,
        name: None,
        optional: false,
        rest: false,
    };
    let check = interner.tuple(vec![wrap(keyof_fn)]);
    let extends = interner.tuple(vec![wrap(TypeId::NEVER)]);
    let conditional = interner.conditional(ConditionalType {
        check_type: check,
        extends_type: extends,
        true_type: TypeId::STRING,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });

    let result = evaluate_type(&interner, conditional);
    assert_eq!(
        result,
        TypeId::STRING,
        "[keyof Fn] extends [never] should pick the true branch (string), \
         got {:?}",
        interner.lookup(result)
    );
}
