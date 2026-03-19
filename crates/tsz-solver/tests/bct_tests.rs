//! Tests for Best Common Type (BCT) inference.
//!
//! These tests exercise the `best_common_type` method on `InferenceContext`,
//! which implements Rule #32: Best Common Type algorithm for determining the
//! most specific type that is a supertype of all candidates.

use super::*;
use crate::inference::infer::InferenceContext;
use crate::intern::TypeInterner;
use crate::types::{FunctionShape, LiteralValue, ParamInfo, PropertyInfo, TupleElement, TypeData};

// =============================================================================
// Identical Types
// =============================================================================

#[test]
fn test_bct_identical_numbers() {
    // BCT of [number, number] is number
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NUMBER, TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_identical_strings() {
    // BCT of [string, string, string] is string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::STRING, TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_identical_booleans() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::BOOLEAN, TypeId::BOOLEAN]);
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_bct_identical_objects() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let name_x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);

    let result = ctx.best_common_type(&[obj, obj, obj]);
    assert_eq!(result, obj);
}

// =============================================================================
// Compatible Types (Literal Types)
// =============================================================================

#[test]
fn test_bct_numeric_literals() {
    // BCT of [1, 2, 3] should widen to number
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);

    let result = ctx.best_common_type(&[one, two, three]);
    // All number literals should widen to number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_string_literals() {
    // BCT of ["a", "b", "c"] should widen to string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");

    let result = ctx.best_common_type(&[a, b, c]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_boolean_literals() {
    // BCT of [true, false] should be boolean
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let t = interner.intern(TypeData::Literal(LiteralValue::Boolean(true)));
    let f = interner.intern(TypeData::Literal(LiteralValue::Boolean(false)));

    let result = ctx.best_common_type(&[t, f]);
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_bct_mixed_string_and_literal() {
    // BCT of [string, "hello"] should be string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");

    let result = ctx.best_common_type(&[TypeId::STRING, hello]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_literal_and_base() {
    // BCT of ["hello", string] should be string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");

    let result = ctx.best_common_type(&[hello, TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Mixed Types
// =============================================================================

#[test]
fn test_bct_string_and_number() {
    // BCT of [string, number] is string | number
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_three_different_primitives() {
    // BCT of [string, number, boolean] is string | number | boolean
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_mixed_literal_and_different_primitive() {
    // BCT of [42, "hello"] should widen to number | string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let forty_two = interner.literal_number(42.0);
    let hello = interner.literal_string("hello");

    let result = ctx.best_common_type(&[forty_two, hello]);
    // Both are different primitive literal types, no common base
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    // The result could be union of widened types or union of literals
    // depending on the BCT algorithm's common base detection
    assert!(
        result == expected || result == interner.union(vec![forty_two, hello]),
        "Expected number | string or 42 | \"hello\", got different type"
    );
}

// =============================================================================
// Subtype Relationship
// =============================================================================

#[test]
fn test_bct_subtype_array() {
    // BCT of [string[], number[]] should be (string | number)[] or string[] | number[]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    let result = ctx.best_common_type(&[string_array, number_array]);
    // Both are arrays, BCT should produce a union
    assert_ne!(result, TypeId::ERROR);
}

#[test]
fn test_bct_subtype_object_with_extra_props() {
    // BCT of [{ x: number }, { x: number, y: string }]
    // { x: number, y: string } <: { x: number }, so BCT should be { x: number }
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);

    let result = ctx.best_common_type(&[base, derived]);
    // derived is subtype of base, so BCT should be base
    assert_eq!(result, base);
}

#[test]
fn test_bct_subtype_both_directions() {
    // BCT of [{ x: number, y: string }, { x: number }]
    // Same test as above but reversed order
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let base = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    let derived = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);

    let result = ctx.best_common_type(&[derived, base]);
    assert_eq!(result, base);
}

// =============================================================================
// Union Members
// =============================================================================

#[test]
fn test_bct_with_union_input() {
    // BCT of [string | number, string]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = ctx.best_common_type(&[union, TypeId::STRING]);
    // string <: string | number, so BCT should be string | number
    assert_eq!(result, union);
}

#[test]
fn test_bct_union_as_supertype() {
    // BCT of [string, number, string | number]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER, union]);
    // Both string and number are subtypes of string | number
    assert_eq!(result, union);
}

// =============================================================================
// Null/Undefined
// =============================================================================

#[test]
fn test_bct_with_null() {
    // BCT of [string, null]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NULL]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_with_undefined() {
    // BCT of [number, undefined]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NUMBER, TypeId::UNDEFINED]);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_with_null_and_undefined() {
    // BCT of [string, null, undefined]
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NULL, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

// =============================================================================
// Empty Input
// =============================================================================

#[test]
fn test_bct_empty_array() {
    // BCT of empty array should be unknown
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[]);
    assert_eq!(result, TypeId::UNKNOWN);
}

// =============================================================================
// Single Type
// =============================================================================

#[test]
fn test_bct_single_string() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_single_literal() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let result = ctx.best_common_type(&[hello]);
    assert_eq!(result, hello);
}

#[test]
fn test_bct_single_object() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let name_x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);

    let result = ctx.best_common_type(&[obj]);
    assert_eq!(result, obj);
}

// =============================================================================
// Never Type Handling
// =============================================================================

#[test]
fn test_bct_never_is_ignored() {
    // Never should not contribute to the BCT
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_all_never() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NEVER, TypeId::NEVER, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_bct_never_mixed() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NEVER, TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Any Type Handling
// =============================================================================

#[test]
fn test_bct_any_absorbs_all() {
    // If any type is 'any', BCT is 'any'
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::ANY, TypeId::NUMBER]);
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_bct_single_any() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::ANY]);
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_bct_any_with_never() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::ANY, TypeId::NEVER]);
    assert_eq!(result, TypeId::ANY);
}

// =============================================================================
// Literal Widening
// =============================================================================

#[test]
fn test_bct_literal_widening_number() {
    // [1, number] should produce number (literal widens to base)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let one = interner.literal_number(1.0);
    let result = ctx.best_common_type(&[one, TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_literal_widening_string() {
    // ["hello", string] should produce string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let result = ctx.best_common_type(&[hello, TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_bct_literal_widening_mixed_literals() {
    // [1, 2] should produce number (all share number base)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);

    let result = ctx.best_common_type(&[one, two]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_fresh_object_literals_preserve_normalized_union() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("a");
    let b_name = interner.intern_string("b");
    let c_name = interner.intern_string("c");

    let obj1 = interner.object_fresh(vec![PropertyInfo::new(a_name, TypeId::NUMBER)]);
    let obj2 = interner.object_fresh(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(b_name, TypeId::STRING),
    ]);
    let obj3 = interner.object_fresh(vec![
        PropertyInfo::new(a_name, TypeId::NUMBER),
        PropertyInfo::new(b_name, TypeId::STRING),
        PropertyInfo::new(c_name, TypeId::BOOLEAN),
    ]);

    let result = ctx.best_common_type(&[obj1, obj2, obj3]);
    let Some(TypeData::Union(list_id)) = interner.lookup(result) else {
        panic!(
            "expected normalized union, got {:?}",
            interner.lookup(result)
        );
    };

    let members = interner.type_list(list_id);
    assert_eq!(
        members.len(),
        3,
        "expected all object-literal candidates to survive"
    );
}

// =============================================================================
// Deduplication
// =============================================================================

#[test]
fn test_bct_dedup() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_dedup_all_same() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NUMBER, TypeId::NUMBER, TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

// =============================================================================
// Function Types
// =============================================================================

#[test]
fn test_bct_different_functions() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let fn1 = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fn2 = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = ctx.best_common_type(&[fn1, fn2]);
    // Different function types should produce a union
    let expected = interner.union(vec![fn1, fn2]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_identical_functions() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = ctx.best_common_type(&[func, func]);
    assert_eq!(result, func);
}

// =============================================================================
// Tuple Types
// =============================================================================

#[test]
fn test_bct_tuples() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let t1 = interner.tuple(vec![
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

    let t2 = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::BOOLEAN,
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

    let result = ctx.best_common_type(&[t1, t2]);
    // Different tuples should produce a union
    let expected = interner.union(vec![t1, t2]);
    assert_eq!(result, expected);
}

// =============================================================================
// Subtype Cache
// =============================================================================

#[test]
fn test_bct_uses_subtype_cache() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    assert!(ctx.subtype_cache.borrow().is_empty());

    let _ = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let cache_size = ctx.subtype_cache.borrow().len();
    assert!(cache_size > 0, "Subtype cache should be populated");

    // Calling again should reuse cache
    let _ = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let cache_size_after = ctx.subtype_cache.borrow().len();
    assert_eq!(cache_size, cache_size_after, "Cache should be reused");
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_bct_void_and_undefined() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::VOID, TypeId::UNDEFINED]);
    // void and undefined are distinct, so result should be a union
    let expected = interner.union(vec![TypeId::VOID, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_with_error_type() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::ERROR, TypeId::STRING]);
    // Error type mixed with string
    assert_ne!(result, TypeId::NEVER);
}

#[test]
fn test_bct_many_duplicates() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let types = vec![
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::STRING,
    ];
    let result = ctx.best_common_type(&types);
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_bct_large_input() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // All identical - should fast-path
    let types: Vec<TypeId> = (0..100).map(|_| TypeId::NUMBER).collect();
    let result = ctx.best_common_type(&types);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_bct_object_and_primitive() {
    // BCT of [{ x: number }, string] should be union
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let name_x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);

    let result = ctx.best_common_type(&[obj, TypeId::STRING]);
    let expected = interner.union(vec![obj, TypeId::STRING]);
    assert_eq!(result, expected);
}
