//! Tests for structural type matching (inference via `infer_from_types`).
//!
//! These tests exercise the `infer_from_types` method on `InferenceContext`,
//! which is the core algorithm for inferring type parameters from function
//! arguments by walking type structures in parallel.

use super::*;
use crate::inference::infer::InferenceContext;
use crate::intern::TypeInterner;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, InferencePriority, ParamInfo, PropertyInfo,
    TupleElement, TypeData, TypeParamInfo,
};

// =============================================================================
// Helper to create a TypeParameter type
// =============================================================================

fn make_type_param(interner: &TypeInterner, name: &str) -> (tsz_common::interner::Atom, TypeId) {
    let atom = interner.intern_string(name);
    let ty = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    }));
    (atom, ty)
}

// =============================================================================
// Simple Matching: T against a concrete type
// =============================================================================

#[test]
fn test_match_number_against_t() {
    // Match `number` against `T` => infers T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::NUMBER, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_match_string_against_t() {
    // Match `string` against `T` => infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::STRING, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_literal_against_t() {
    // Match `"hello"` against `T` => infers T = "hello" (literal string)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.infer_from_types(hello, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Fresh literal should widen to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_boolean_against_t() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(
        TypeId::BOOLEAN,
        t_type,
        InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_match_same_type_no_inference() {
    // If source == target (same TypeId), no inference happens
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, _t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    // Matching number against number should be a no-op
    ctx.infer_from_types(
        TypeId::NUMBER,
        TypeId::NUMBER,
        InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    // T should remain unresolved since we didn't match against T
    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.probe(var_t);
    assert!(result.is_none());
}

// =============================================================================
// Object Matching
// =============================================================================

#[test]
fn test_match_object_property() {
    // Match `{ x: string }` against `{ x: T }` => infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_object_multiple_properties() {
    // Match `{ x: string, y: number }` against `{ x: T, y: U }`
    // => infers T = string, U = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
    ]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_match_object_extra_source_properties() {
    // Match `{ x: string, y: number, z: boolean }` against `{ x: T }`
    // => infers T = string (extra props are ignored)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
        PropertyInfo::new(name_z, TypeId::BOOLEAN),
    ]);
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_object_missing_source_property() {
    // Match `{ x: string }` against `{ x: T, y: U }`
    // => infers T = string, U stays unresolved
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result_t, TypeId::STRING);

    // U should be unresolved
    let result_u = ctx.probe(var_u);
    assert!(result_u.is_none());
}

// =============================================================================
// Function Matching
// =============================================================================

#[test]
fn test_match_function_param_and_return() {
    // Match `(n: number) => string` against `(x: T) => U`
    // => infers T = number (contravariant), U = string (covariant)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("n")),
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

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    // Parameters are contravariant: the inference walks target<->source swapped,
    // so T gets an upper bound of number
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result_t, TypeId::NUMBER);

    let result_u = ctx.resolve_with_constraints(var_u).unwrap();
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_match_function_multiple_params() {
    // Match `(a: string, b: number) => boolean` against `(x: T, y: U) => V`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let (v_name, v_type) = make_type_param(&interner, "V");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);
    let _var_v = ctx.fresh_type_param(v_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: v_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    let var_v = ctx.find_type_param(v_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
    assert_eq!(
        ctx.resolve_with_constraints(var_v).unwrap(),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_match_function_return_only() {
    // Match `() => number` against `() => T` => T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// Array Matching
// =============================================================================

#[test]
fn test_match_array_element_type() {
    // Match `number[]` against `T[]` => infers T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.array(TypeId::NUMBER);
    let target = interner.array(t_type);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_array_of_objects() {
    // Match `{ x: string }[]` against `T[]` => T = { x: string }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);

    let source = interner.array(obj);
    let target = interner.array(t_type);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), obj);
}

// =============================================================================
// Tuple Matching
// =============================================================================

#[test]
fn test_match_tuple_elements() {
    // Match `[string, number]` against `[T, U]`
    // => T = string, U = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.tuple(vec![
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

    let target = interner.tuple(vec![
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_tuple_single_element() {
    // Match `[boolean]` against `[T]`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);

    let target = interner.tuple(vec![TupleElement {
        type_id: t_type,
        name: None,
        optional: false,
        rest: false,
    }]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(var_t).unwrap(),
        TypeId::BOOLEAN
    );
}

// =============================================================================
// Union Matching
// =============================================================================

#[test]
fn test_match_source_union_against_target_union() {
    // Match `string | number` against `T | U`
    // The union-to-union inference should handle this
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Target is just T (not a union of params) - source union against T
    ctx.infer_from_types(source, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // T should be inferred as string | number
    assert_eq!(result, source);
}

#[test]
fn test_match_against_union_target_with_fixed_members() {
    // Match `string` against `T | undefined`
    // T should be inferred as string (undefined is fixed)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let target = interner.union(vec![t_type, TypeId::UNDEFINED]);

    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Nested Matching
// =============================================================================

#[test]
fn test_match_nested_array_in_object() {
    // Match `{ items: number[] }` against `{ items: T[] }`
    // => T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_items = interner.intern_string("items");

    let source = interner.object(vec![PropertyInfo::new(
        name_items,
        interner.array(TypeId::NUMBER),
    )]);
    let target = interner.object(vec![PropertyInfo::new(name_items, interner.array(t_type))]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_nested_object_in_object() {
    // Match `{ inner: { value: boolean } }` against `{ inner: { value: T } }`
    // => T = boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_inner = interner.intern_string("inner");
    let name_value = interner.intern_string("value");

    let inner_source = interner.object(vec![PropertyInfo::new(name_value, TypeId::BOOLEAN)]);
    let source = interner.object(vec![PropertyInfo::new(name_inner, inner_source)]);

    let inner_target = interner.object(vec![PropertyInfo::new(name_value, t_type)]);
    let target = interner.object(vec![PropertyInfo::new(name_inner, inner_target)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(var_t).unwrap(),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_match_function_returning_array() {
    // Match `() => string[]` against `() => T[]` => T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: interner.array(TypeId::STRING),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: interner.array(t_type),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
}

// =============================================================================
// Contravariant Matching
// =============================================================================

#[test]
fn test_match_contravariant_parameter() {
    // In function parameter position, inference is contravariant.
    // Match `(x: string) => void` against `(x: T) => void`
    // T gets an upper bound from contravariant position.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    // In contravariant position, T gets string as an upper bound
    // (because infer_functions swaps target and source for params)
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// No Match Cases
// =============================================================================

#[test]
fn test_match_different_structures_no_panic() {
    // Match `string` against `{ x: T }` - no structural match, no panic
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    // String against object - no structural match
    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T should remain unresolved
    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.probe(var_t);
    assert!(result.is_none());
}

#[test]
fn test_match_number_against_function_no_panic() {
    // Match `number` against `(x: T) => U` - no structural match
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(TypeId::NUMBER, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Both should remain unresolved
    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    assert!(ctx.probe(var_t).is_none());
    assert!(ctx.probe(var_u).is_none());
}

// =============================================================================
// Partial Match
// =============================================================================

#[test]
fn test_match_partial_object_properties() {
    // Match `{ x: string }` against `{ x: T, y: U }`
    // T should be inferred, U should remain unresolved
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert!(ctx.probe(var_u).is_none());
}

// =============================================================================
// Readonly Type Matching
// =============================================================================

#[test]
fn test_match_readonly_unwrap() {
    // Match `readonly number[]` against `readonly T[]`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source_inner = interner.array(TypeId::NUMBER);
    let source = interner.intern(TypeData::ReadonlyType(source_inner));

    let target_inner = interner.array(t_type);
    let target = interner.intern(TypeData::ReadonlyType(target_inner));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_mutable_source_against_readonly_target() {
    // Match `number[]` against `readonly T[]`
    // mutable source is compatible with readonly target
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.array(TypeId::NUMBER);
    let target_inner = interner.array(t_type);
    let target = interner.intern(TypeData::ReadonlyType(target_inner));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// NoInfer Matching
// =============================================================================

#[test]
fn test_match_noinfer_blocks_inference() {
    // Match `string` against `NoInfer<T>` - should NOT infer T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let target = interner.intern(TypeData::NoInfer(t_type));

    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T should remain unresolved due to NoInfer
    let var_t = ctx.find_type_param(t_name).unwrap();
    assert!(ctx.probe(var_t).is_none());
}

// =============================================================================
// Intersection Matching
// =============================================================================

#[test]
fn test_match_intersection_target() {
    // Match `{ x: string, y: number }` against `{ x: T } & { y: U }`
    // Each member of the intersection should be tried
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
    ]);

    let part1 = interner.object(vec![PropertyInfo::new(name_x, t_type)]);
    let part2 = interner.object(vec![PropertyInfo::new(name_y, u_type)]);
    let target = interner.intersection(vec![part1, part2]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// Index Access Matching
// =============================================================================

#[test]
fn test_match_index_access() {
    // Match `IndexAccess(A, B)` against `IndexAccess(T, U)`
    // => T = A, U = B
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.intern(TypeData::IndexAccess(TypeId::STRING, TypeId::NUMBER));
    let target = interner.intern(TypeData::IndexAccess(t_type, u_type));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// Upper Bound (Source Position) Matching
// =============================================================================

#[test]
fn test_match_type_param_in_source_adds_upper_bound() {
    // When T appears as source, it becomes an upper bound: T <: target
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    // Infer T <: string (T is the source)
    ctx.infer_from_types(t_type, TypeId::STRING, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let constraints = ctx.get_constraints(var_t).unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

// =============================================================================
// Multiple Candidates
// =============================================================================

#[test]
fn test_match_multiple_sources_same_param() {
    // Two inferences into T: T = string and T = number
    // tsc unions candidates with same priority: result is string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::STRING, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();
    ctx.infer_from_types(TypeId::NUMBER, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple candidates at the same priority level
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected_union);
}

// =============================================================================
// Callable Matching
// =============================================================================

#[test]
fn test_match_callable_signatures() {
    // Match a callable with call signature against another with T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        symbol: None,
        is_abstract: false,
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        symbol: None,
        is_abstract: false,
        ..Default::default()
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
}
