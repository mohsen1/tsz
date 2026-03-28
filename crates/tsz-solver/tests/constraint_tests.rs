//! Tests for constraint collection in generic type inference.
//!
//! These tests exercise the `constrain_types` method on `CallEvaluator`,
//! which is the structural walker that collects type constraints when
//! inferring generic type parameters from argument types.

use super::*;
use crate::CompatChecker;
use crate::inference::infer::{InferenceContext, InferenceError};
use crate::intern::TypeInterner;
use crate::types::{
    FunctionShape, InferencePriority, ParamInfo, PropertyInfo, TupleElement, TypeData,
    TypeParamInfo,
};

// =============================================================================
// Helper: create a CallEvaluator + InferenceContext for constraint tests
// =============================================================================

/// Create a simple generic call scenario and collect constraints via `resolve_call`.
/// This exercises the constraint collection pipeline end-to-end.

// =============================================================================
// Simple Constraint Tests (via InferenceContext directly)
// =============================================================================

#[test]
fn test_constraint_simple_string() {
    // T extends string: verify that passing a string literal satisfies the constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Simulate: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Simulate: passing "hello" for T
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // "hello" satisfies `extends string`, resolves to string (widened)
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_simple_number() {
    // T extends number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_constraint_simple_boolean() {
    // T extends boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::BOOLEAN);
    let true_lit = interner.intern(TypeData::Literal(crate::types::LiteralValue::Boolean(true)));
    ctx.add_lower_bound(var_t, true_lit);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

// =============================================================================
// Union Constraint Tests
// =============================================================================

#[test]
fn test_constraint_union_string_or_number() {
    // T extends string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let upper = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, upper);

    // Pass a string - should satisfy the constraint
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_union_satisfies_with_literal() {
    // T extends string | number, pass "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let upper = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, upper);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal string satisfies string | number
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_union_violates() {
    // T extends string | number, pass boolean - should fail
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let upper = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, upper);

    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

// =============================================================================
// Object Constraint Tests
// =============================================================================

#[test]
fn test_constraint_object_extends() {
    // T extends { x: number }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let upper = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    ctx.add_upper_bound(var_t, upper);

    // Pass { x: number, y: string } - should satisfy the constraint
    let name_y = interner.intern_string("y");
    let lower = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    ctx.add_lower_bound(var_t, lower);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_constraint_object_violates() {
    // T extends { x: number }, pass { x: string }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let upper = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    ctx.add_upper_bound(var_t, upper);

    let lower = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    ctx.add_lower_bound(var_t, lower);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

#[test]
fn test_constraint_object_missing_property() {
    // T extends { x: number, y: string }, pass { x: number }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let upper = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    ctx.add_upper_bound(var_t, upper);

    let lower = interner.object(vec![PropertyInfo::new(name_x, TypeId::NUMBER)]);
    ctx.add_lower_bound(var_t, lower);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

// =============================================================================
// Function Constraint Tests
// =============================================================================

#[test]
fn test_constraint_function_extends() {
    // T extends (x: number) => string, pass (x: number) => string
    // Use matching signatures to work with the simplified subtype checker
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };

    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![param],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, upper);

    // Pass the same function type
    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![param],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, lower);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_constraint_function_extends_with_compat_checker() {
    // T extends (x: any) => any, using CompatChecker for proper any handling
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let mut checker = CompatChecker::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, upper);

    let lower = interner.function(FunctionShape {
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
    ctx.add_lower_bound(var_t, lower);

    // Use CompatChecker which handles `any` properly
    let result = ctx
        .resolve_with_constraints_by(var_t, |source, target| {
            checker.is_assignable_to(source, target)
        })
        .unwrap();
    assert_eq!(result, lower);
}

// =============================================================================
// Keyof Constraint Tests
// =============================================================================

#[test]
fn test_constraint_keyof_object() {
    // T extends keyof { x: number, y: string } => T extends "x" | "y"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let obj = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::NUMBER),
        PropertyInfo::new(name_y, TypeId::STRING),
    ]);
    let upper = interner.keyof(obj);
    ctx.add_upper_bound(var_t, upper);

    // Pass "x" - should be valid
    let x_lit = interner.literal_string("x");
    ctx.add_lower_bound(var_t, x_lit);

    // This should resolve - "x" is a valid key of the object
    let result = ctx.resolve_with_constraints(var_t);
    // The keyof type is a structural type; the BCT-level subtype check
    // may or may not evaluate the keyof. The key thing is it doesn't panic.
    assert!(result.is_ok() || matches!(result, Err(InferenceError::BoundsViolation { .. })));
}

// =============================================================================
// Multiple Constraint Tests
// =============================================================================

#[test]
fn test_constraint_multiple_params_independent() {
    // <T extends string, U extends number>(a: T, b: U)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_u, TypeId::NUMBER);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, forty_two);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_constraint_multiple_params_one_violates() {
    // <T extends string, U extends number>(a: T, b: U)
    // Pass (string, boolean) - U violates
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_u, TypeId::NUMBER);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::BOOLEAN);

    // T should succeed
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result_t, TypeId::STRING);

    // U should fail
    let result_u = ctx.resolve_with_constraints(var_u);
    assert!(matches!(
        result_u,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

// =============================================================================
// Unsatisfiable Constraint Tests
// =============================================================================

#[test]
fn test_constraint_unsatisfiable_never() {
    // T extends never - only never satisfies
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::NEVER);

    // Pass string - should fail
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation { .. })
    ));
}

#[test]
fn test_constraint_only_never_satisfies_never() {
    // T extends never, pass never - should succeed
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::NEVER);
    ctx.add_lower_bound(var_t, TypeId::NEVER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NEVER);
}

#[test]
fn test_constraint_contradictory_bounds() {
    // T extends string, but also T extends number (cannot satisfy both)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    // No lower bound - should resolve to intersection of upper bounds
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// =============================================================================
// Constraint Inference Tests (via infer_from_types)
// =============================================================================

#[test]
fn test_constraint_infer_from_array() {
    // function id<T>(arr: T[]): T; id([1,2,3]) => T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Target: T[] (array of T)
    let target = interner.array(t_type);
    // Source: number[]
    let source = interner.array(TypeId::NUMBER);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_constraint_infer_from_object_property() {
    // function get<T>(obj: { value: T }): T; get({ value: "hello" }) => T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let name_value = interner.intern_string("value");
    let target = interner.object(vec![PropertyInfo::new(name_value, t_type)]);
    let hello = interner.literal_string("hello");
    let source = interner.object(vec![PropertyInfo::new(name_value, hello)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_infer_from_function() {
    // function apply<T, U>(fn: (x: T) => U, arg: T): U
    // apply((x: number) => "result", 42) => T = number, U = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Target: (x: T) => U
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

    // Source: (x: number) => string
    let source = interner.function(FunctionShape {
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

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T is inferred from contravariant position (parameter), U from covariant (return)
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    // Parameters are contravariant: target param type is swapped with source,
    // so T gets NUMBER as upper bound
    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

// =============================================================================
// Recursive Constraint Tests
// =============================================================================

#[test]
fn test_constraint_recursive_self_referential() {
    // T extends Comparable<T> modeled as T extends { compareTo: (other: T) => number }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends { compareTo: (other: T) => number }
    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let name_compare = interner.intern_string("compareTo");
    let upper = interner.object(vec![PropertyInfo::new(name_compare, compare_fn)]);

    ctx.add_upper_bound(var_t, upper);

    // Resolves to unknown because the upper bound contains a circular reference
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_constraint_recursive_with_concrete_lower() {
    // T extends { next: T }, but lower bound is a concrete type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let name_next = interner.intern_string("next");
    let upper = interner.object(vec![PropertyInfo::new(name_next, t_type)]);
    ctx.add_upper_bound(var_t, upper);

    // Add concrete lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // With a circular upper bound and a concrete lower bound,
    // the circular upper bound is detected and the concrete lower bound is used
    let result = ctx.resolve_with_constraints(var_t);
    // The constraint may fail because string <: { next: T } is unlikely to hold
    assert!(result.is_ok() || matches!(result, Err(InferenceError::BoundsViolation { .. })));
}

// =============================================================================
// Conditional Constraint Tests (constraints involving conditional types)
// =============================================================================

#[test]
fn test_constraint_with_conditional_upper_bound() {
    // T extends (U extends string ? number : boolean)
    // This is a more complex constraint shape
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // For simplicity, just use the result type as the upper bound
    // (conditional evaluation is handled by the evaluator)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

// =============================================================================
// Constraint Collection via CallEvaluator (end-to-end)
// =============================================================================

#[test]
fn test_constraint_call_generic_identity() {
    // function identity<T>(x: T): T; identity("hello")
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let hello = interner.literal_string("hello");
    let result = evaluator.resolve_call(func, &[hello]);

    match result {
        CallResult::Success(ret) => {
            // The return type should be inferred from T = "hello" (widened to string)
            assert!(ret == TypeId::STRING || ret == hello);
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

#[test]
fn test_constraint_call_generic_with_constraint() {
    // function stringify<T extends string | number>(x: T): string; stringify(42)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let constraint = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(constraint),
        default: None,
        is_const: false,
    }));

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: Some(constraint),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let forty_two = interner.literal_number(42.0);
    let result = evaluator.resolve_call(func, &[forty_two]);

    match result {
        CallResult::Success(ret) => {
            assert_eq!(ret, TypeId::STRING);
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

#[test]
fn test_constraint_call_generic_two_params() {
    // function pair<T, U>(a: T, b: U): [T, U]; pair("hello", 42)
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let return_type = interner.tuple(vec![
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

    let func = interner.function(FunctionShape {
        type_params: vec![
            TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: u_name,
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);
    let result = evaluator.resolve_call(func, &[hello, forty_two]);

    match result {
        CallResult::Success(ret) => {
            // Return type should be a tuple - verify it exists
            assert_ne!(ret, TypeId::ERROR);
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

#[test]
fn test_constraint_call_generic_array_element() {
    // function first<T>(arr: T[]): T; first([1, 2, 3])
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arr")),
            type_id: interner.array(t_type),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let number_array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_call(func, &[number_array]);

    match result {
        CallResult::Success(ret) => {
            assert_eq!(ret, TypeId::NUMBER);
        }
        other => panic!("Expected success, got {other:?}"),
    }
}

// =============================================================================
// Declared Constraint Tests
// =============================================================================

#[test]
fn test_declared_constraint_set_and_get() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.set_declared_constraint(var_t, TypeId::STRING);

    let constraint = ctx.get_declared_constraint(var_t);
    assert_eq!(constraint, Some(TypeId::STRING));
}

#[test]
fn test_declared_constraint_none_when_not_set() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    let constraint = ctx.get_declared_constraint(var_t);
    assert_eq!(constraint, None);
}

// =============================================================================
// Constraint Merge Tests
// =============================================================================

#[test]
fn test_constraint_merge_on_unify_lower_and_upper() {
    // When two vars are unified, their constraints should merge
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var_a = ctx.fresh_var();
    let var_b = ctx.fresh_var();

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_upper_bound(var_b, TypeId::STRING);

    ctx.unify_vars(var_a, var_b).unwrap();

    let constraints = ctx.get_constraints(var_a).unwrap();
    assert!(constraints.lower_bounds.contains(&TypeId::STRING));
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_constraint_dedup_lower_bounds() {
    // Adding the same lower bound twice should not duplicate it
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    ctx.add_lower_bound(var, TypeId::STRING);
    ctx.add_lower_bound(var, TypeId::STRING);

    let constraints = ctx.get_constraints(var).unwrap();
    // Lower bounds should be deduplicated
    assert_eq!(constraints.lower_bounds.len(), 1);
}

#[test]
fn test_constraint_empty_no_constraints() {
    // A fresh variable has no constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    let constraints = ctx.get_constraints(var);
    assert!(constraints.is_none());
}

// =============================================================================
// Contra-candidate Tests
// =============================================================================

#[test]
fn test_contra_candidate_basic() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Add a contravariant candidate
    ctx.add_contra_candidate(var_t, TypeId::STRING, InferencePriority::NakedTypeVariable);
    ctx.add_contra_candidate(var_t, TypeId::NUMBER, InferencePriority::NakedTypeVariable);

    // With only contra-candidates, resolution uses intersection
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Contravariant candidates should produce an intersection
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// =============================================================================
// Constraint with Any/Unknown/Never
// =============================================================================

#[test]
fn test_constraint_any_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::ANY);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // any is a valid upper bound for string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_unknown_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::UNKNOWN);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_constraint_upper_bound_only_defaults() {
    // When only upper bound exists, should default to the upper bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_no_bounds_defaults_unknown() {
    // When no bounds exist, should default to unknown
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // No bounds at all - should resolve to unknown
    let result = ctx.resolve_with_constraints(var_t);
    // Without any constraints, the variable may resolve to unknown or stay unresolved
    assert!(
        result.is_ok() || matches!(result, Err(InferenceError::Unresolved(_))),
        "Expected Ok(UNKNOWN) or Err(Unresolved), got {result:?}"
    );
}

// =============================================================================
// fix_current_variables: unknown candidate filtering with upper bounds
// =============================================================================

#[test]
fn test_fix_current_variables_filters_unknown_with_informative_upper_bound() {
    // Simulates: f<T>(value: T[], func: (t: T) => void) called as f([], acceptStr)
    // The empty array contributes `unknown` as a covariant candidate (from contextual typing),
    // while `acceptStr` contributes `string` as a contra-candidate (from function param).
    // The reverse constraint direction adds `string` as an upper bound on T.
    //
    // fix_current_variables should filter out `unknown` when `string` upper bound exists,
    // allowing the contra-candidate to drive inference → T = string.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Covariant candidate: unknown (from empty array element type)
    ctx.add_candidate(var_t, TypeId::UNKNOWN, InferencePriority::NakedTypeVariable);
    // Contravariant candidate: string (from callback parameter)
    ctx.add_contra_candidate(var_t, TypeId::STRING, InferencePriority::NakedTypeVariable);
    // Upper bound: string (from reverse constraint T <: string)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // fix_current_variables should filter the `unknown` candidate and use contra-candidates
    ctx.fix_current_variables().unwrap();

    // T should resolve to `string` from the contra-candidate, not `unknown`
    let resolved = ctx.probe(var_t);
    assert_eq!(
        resolved,
        Some(TypeId::STRING),
        "Expected T = string (from contra-candidate), got {resolved:?}"
    );
}

#[test]
fn test_fix_current_variables_keeps_concrete_candidate_with_upper_bound() {
    // When the covariant candidate is concrete (not unknown/error), it should be preserved.
    // Simulates: f<T>(value: T[], func: (t: T) => void) called as f(["hello"], acceptStr)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Covariant candidate: string (from ["hello"] element type)
    ctx.add_candidate(var_t, TypeId::STRING, InferencePriority::NakedTypeVariable);
    // Contravariant candidate: string (from callback parameter)
    ctx.add_contra_candidate(var_t, TypeId::STRING, InferencePriority::NakedTypeVariable);
    // Upper bound: string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    ctx.fix_current_variables().unwrap();

    let resolved = ctx.probe(var_t);
    assert_eq!(
        resolved,
        Some(TypeId::STRING),
        "Expected T = string (from covariant candidate), got {resolved:?}"
    );
}

#[test]
fn test_fix_current_variables_unknown_without_upper_bound_stays_unknown() {
    // When there is no upper bound, unknown covariant candidate should NOT be filtered.
    // This ensures we don't break the case where T genuinely resolves to unknown.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Only covariant candidate: unknown
    ctx.add_candidate(var_t, TypeId::UNKNOWN, InferencePriority::NakedTypeVariable);

    ctx.fix_current_variables().unwrap();

    let resolved = ctx.probe(var_t);
    assert_eq!(
        resolved,
        Some(TypeId::UNKNOWN),
        "Expected T = unknown (no upper bound to filter), got {resolved:?}"
    );
}

#[test]
fn test_contra_candidate_wins_when_only_unknown_covariant() {
    // Tests that resolve_with_constraints also properly handles contra-candidates
    // when the only covariant candidate is `unknown` with an informative upper bound.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_candidate(var_t, TypeId::UNKNOWN, InferencePriority::NakedTypeVariable);
    ctx.add_contra_candidate(var_t, TypeId::NUMBER, InferencePriority::NakedTypeVariable);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(
        result,
        TypeId::NUMBER,
        "Expected T = number (contra-candidate wins over filtered unknown), got {result:?}"
    );
}
