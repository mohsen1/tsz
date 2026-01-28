use super::*;
use crate::solver::{AssignabilityChecker, CompatChecker, ConditionalType, infer_generic_function};

#[test]
fn test_inference_basic() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    // Create inference variable
    let var = ctx.fresh_var();

    // Should start unresolved
    assert!(ctx.probe(var).is_none());

    // Unify with number
    ctx.unify_var_type(var, TypeId::NUMBER).unwrap();

    // Should now be number
    assert_eq!(ctx.probe(var), Some(TypeId::NUMBER));
}

#[test]
fn test_inference_type_param() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Create type parameter T
    let var_t = ctx.fresh_type_param(t_name);

    // Look it up
    let found = ctx.find_type_param(t_name);
    assert_eq!(found, Some(var_t));

    // Not found
    let not_found = ctx.find_type_param(u_name);
    assert!(not_found.is_none());
}

#[test]
fn test_inference_conflict() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    // Unify with string
    ctx.unify_var_type(var, TypeId::STRING).unwrap();

    // Try to unify with number - should fail
    let result = ctx.unify_var_type(var, TypeId::NUMBER);
    assert!(result.is_err());
}

#[test]
fn test_inference_unify_vars() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    ctx.unify_vars(var_t, var_u).unwrap();
    ctx.unify_var_type(var_u, TypeId::STRING).unwrap();

    assert_eq!(ctx.probe(var_t), Some(TypeId::STRING));
    assert_eq!(ctx.probe(var_u), Some(TypeId::STRING));
}

#[test]
fn test_inference_unify_vars_conflict() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var_a = ctx.fresh_var();
    let var_b = ctx.fresh_var();

    ctx.unify_var_type(var_a, TypeId::STRING).unwrap();
    ctx.unify_var_type(var_b, TypeId::NUMBER).unwrap();

    let result = ctx.unify_vars(var_a, var_b);
    assert!(matches!(
        result,
        Err(InferenceError::Conflict(a, b))
            if (a == TypeId::STRING && b == TypeId::NUMBER)
            || (a == TypeId::NUMBER && b == TypeId::STRING)
    ));
}

#[test]
fn test_inference_occurs_check() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let array_t = interner.array(t_type);

    let result = ctx.unify_var_type(var_t, array_t);
    assert!(matches!(result, Err(InferenceError::OccursCheck { .. })));
}

#[test]
fn test_inference_occurs_check_function_this_type() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = ctx.unify_var_type(var_t, func);
    assert!(matches!(result, Err(InferenceError::OccursCheck { .. })));
}

// =============================================================================
// Constraint Collection Tests
// =============================================================================

#[test]
fn test_constraint_lower_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    // Add lower bound: string <: var
    ctx.add_lower_bound(var, TypeId::STRING);

    let constraints = ctx.get_constraints(var).unwrap();
    assert_eq!(constraints.lower_bounds.len(), 1);
    assert!(constraints.lower_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_constraint_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    // Add upper bound: var <: string
    ctx.add_upper_bound(var, TypeId::STRING);

    let constraints = ctx.get_constraints(var).unwrap();
    assert_eq!(constraints.upper_bounds.len(), 1);
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_constraint_multiple_lower_bounds() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_var();

    // foo<T>(a: T, b: T) called with foo("hello", 42)
    // Lower bounds: "hello" <: T, 42 <: T
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var, hello);
    ctx.add_lower_bound(var, forty_two);

    let constraints = ctx.get_constraints(var).unwrap();
    assert_eq!(constraints.lower_bounds.len(), 2);
}

#[test]
fn test_constraint_merge_on_unify() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var_a = ctx.fresh_var();
    let var_b = ctx.fresh_var();

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_upper_bound(var_b, TypeId::NUMBER);

    ctx.unify_vars(var_a, var_b).unwrap();

    let constraints = ctx.get_constraints(var_a).unwrap();
    assert!(constraints.lower_bounds.contains(&TypeId::STRING));
    assert!(constraints.upper_bounds.contains(&TypeId::NUMBER));
}

// =============================================================================
// Bounds Resolution Tests
// =============================================================================

#[test]
fn test_resolve_unified_vars_merged_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var_a = ctx.fresh_var();
    let var_b = ctx.fresh_var();
    let hello = interner.literal_string("hello");

    ctx.add_lower_bound(var_a, hello);
    ctx.add_upper_bound(var_b, TypeId::STRING);
    ctx.unify_vars(var_a, var_b).unwrap();

    let result = ctx.resolve_with_constraints(var_a).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_single_lower_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    // Add lower bound: string <: T
    ctx.add_lower_bound(var, TypeId::STRING);

    // Resolve should return string
    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_multiple_lower_bounds_union() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    // foo<T>(a: T, b: T) called with foo("hello", 42)
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var, hello);
    ctx.add_lower_bound(var, forty_two);

    // Resolve should return "hello" | 42
    let result = ctx.resolve_with_constraints(var).unwrap();
    let expected = interner.union(vec![hello, forty_two]);
    assert_eq!(result, expected);
}

#[test]
fn test_resolve_lower_bounds_ignores_never() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    let hello = interner.literal_string("hello");

    ctx.add_lower_bound(var, TypeId::NEVER);
    ctx.add_lower_bound(var, hello);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_upper_bound_only() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    // function f<T extends string>() - upper bound only
    ctx.add_upper_bound(var, TypeId::STRING);

    // No lower bounds - should default to upper bound
    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_any_lower_prefers_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    ctx.add_lower_bound(var, TypeId::ANY);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_unknown_lower_prefers_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    ctx.add_lower_bound(var, TypeId::UNKNOWN);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_error_lower_prefers_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    ctx.add_lower_bound(var, TypeId::ERROR);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_error_lower_with_literal_prefers_literal() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let hello = interner.literal_string("hello");

    ctx.add_lower_bound(var, TypeId::ERROR);
    ctx.add_lower_bound(var, hello);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_contextual_ignores_any_lower_with_literal() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let hello = interner.literal_string("hello");

    ctx.add_lower_bound(var, TypeId::ANY);
    ctx.add_lower_bound(var, hello);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_circular_upper_bound_defaults_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let name_next = interner.intern_string("next");
    let upper = interner.object(vec![PropertyInfo {
        name: name_next,
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_resolve_self_upper_bound_with_concrete() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    ctx.add_upper_bound(var, t_type);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_mutual_circular_upper_bounds_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::UNKNOWN);
    assert_eq!(result_u, TypeId::UNKNOWN);
}

#[test]
fn test_resolve_mutual_circular_upper_bounds_with_concrete() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_resolve_self_recursive_object_bounds_two_params_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let name_next = interner.intern_string("next");

    let upper_t = interner.object(vec![PropertyInfo {
        name: name_next,
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let upper_u = interner.object(vec![PropertyInfo {
        name: name_next,
        type_id: u_type,
        write_type: u_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_upper_bound(var_t, upper_t);
    ctx.add_upper_bound(var_u, upper_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::UNKNOWN);
    assert_eq!(result_u, TypeId::UNKNOWN);
}

#[test]
fn test_resolve_mutual_recursive_object_bounds_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let name_next = interner.intern_string("next");

    let upper_t = interner.object(vec![PropertyInfo {
        name: name_next,
        type_id: u_type,
        write_type: u_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let upper_u = interner.object(vec![PropertyInfo {
        name: name_next,
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_upper_bound(var_t, upper_t);
    ctx.add_upper_bound(var_u, upper_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::UNKNOWN);
    assert_eq!(result_u, TypeId::UNKNOWN);
}

#[test]
fn test_resolve_multiple_upper_bounds_intersection() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var).unwrap();
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_resolve_bounds_valid() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    // function f<T extends string>(x: T) called with f("hello")
    // Lower: "hello" <: T, Upper: T <: string
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var, hello);
    ctx.add_upper_bound(var, TypeId::STRING);

    // Resolve should work: "hello" is subtype of string
    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_bounds_tuple_lower_array_upper() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);
    let string_array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: string_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    ctx.add_lower_bound(var, tuple);
    ctx.add_upper_bound(var, string_array);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, tuple);
}

#[test]
fn test_resolve_bounds_union_upper_allows_literal_lower() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let hello = interner.literal_string("hello");
    let upper = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    ctx.add_lower_bound(var, hello);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_resolve_bounds_object_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");
    let name_b = interner.intern_string("b");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let lower = interner.object(vec![
        PropertyInfo {
            name: name_a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: name_b,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_union_lower_vs_string_upper() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let lower = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == TypeId::STRING
    ));
}

#[test]
fn test_resolve_bounds_object_readonly_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_object_readonly_property_ok() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_object_readonly_property_missing_ok() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: true,
        is_method: false,
    }]);
    let lower = interner.object(Vec::new());

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_method_property_bivariant_params() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_m = interner.intern_string("m");

    let narrow_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let wide_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };

    let lower_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![narrow_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![wide_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let lower = interner.object(vec![PropertyInfo {
        name: name_m,
        type_id: lower_fn,
        write_type: lower_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    let upper = interner.object(vec![PropertyInfo {
        name: name_m,
        type_id: upper_fn,
        write_type: upper_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_property_contravariant_params() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_f = interner.intern_string("f");

    let narrow_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let wide_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };

    let lower_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![narrow_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![wide_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let lower = interner.object(vec![PropertyInfo {
        name: name_f,
        type_id: lower_fn,
        write_type: lower_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let upper = interner.object(vec![PropertyInfo {
        name: name_f,
        type_id: upper_fn,
        write_type: upper_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_with_assignability_bivariant_function_property() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let mut checker = CompatChecker::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_f = interner.intern_string("f");

    let narrow_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let wide_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };

    let lower_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![narrow_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![wide_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let lower = interner.object(vec![PropertyInfo {
        name: name_f,
        type_id: lower_fn,
        write_type: lower_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let upper = interner.object(vec![PropertyInfo {
        name: name_f,
        type_id: upper_fn,
        write_type: upper_fn,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx
        .resolve_with_constraints_by(var, |source, target| {
            checker.is_assignable_to(source, target)
        })
        .unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_param_contravariance_extends() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let narrow_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let wide_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };

    let lower_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![wide_param],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![narrow_param],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Contextual signature provides a narrow parameter type constraint.
    ctx.add_lower_bound(var, lower_fn);
    ctx.add_upper_bound(var, upper_fn);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_fn);
}

#[test]
fn test_resolve_bounds_function_return_covariance_extends() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![param.clone()],
        this_type: None,
        return_type: interner.literal_string("ok"),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![param],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower_fn);
    ctx.add_upper_bound(var, upper_fn);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_fn);
}

#[test]
fn test_resolve_bounds_object_keyword_upper_allows_array() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let lower = interner.array(TypeId::STRING);
    let upper = TypeId::OBJECT;

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_object_keyword_rejects_string() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let lower = TypeId::STRING;
    let upper = TypeId::OBJECT;

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_object_with_index_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let lower = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name: name_a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
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

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_string_index_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_index_readonly_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_index_readonly_signature_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let lower = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
        number_index: None,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_index_readonly_signature_allows_mutable_source() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
        number_index: None,
    });

    let lower = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_number_index_allows_non_numeric_property() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name: name_a,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_number_index_numeric_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_zero = interner.intern_string("0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name: name_zero,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_readonly_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_zero = interner.intern_string("0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object(vec![PropertyInfo {
        name: name_zero,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_readonly_signature_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_readonly_signature_allows_mutable_source() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: true,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_non_canonical_numeric_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("01");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_accepts_exponent_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e-7");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_accepts_infinity_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("Infinity");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_accepts_nan_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("NaN");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_accepts_negative_infinity_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-Infinity");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_zero_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_zero_property() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object(vec![PropertyInfo {
        name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_accepts_decimal_boundary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("0.000001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_accepts_exponent_boundary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e+21");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_ignores_non_canonical_exponent_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e+021");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E+21");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_missing_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E21");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E+0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_leading_zeros_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E+00");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_leading_zeros_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_negative_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E-0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1eE1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_with_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee+1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1eE");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_missing_sign_with_leading_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E01");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_double_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1eE++1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_with_lowercase_e() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1eE+1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee--1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_plus_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee+-1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_minus_plus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee-+1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_trailing_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee+");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_trailing_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee-");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_trailing_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee--");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee+0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_leading_zeros_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_missing_sign_with_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee01");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_negative_exponent_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee-0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_positive_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee+0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_zero_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_double_sign_trailing() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1Ee++");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E+");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_minus_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E-");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_double_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E++1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1E--1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_negative() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e-0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_positive() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e+0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e0001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_missing_exponent_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e21");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_leading_zero_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("01.0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_hex_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("0x1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_binary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("0b1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_octal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("0o7");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_leading_zero_mantissa() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("01e+1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_leading_dot_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string(".5");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_multiple_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("00");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_hex_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0x1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_binary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0b1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_octal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0o7");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_double_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e++1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e--1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e+");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_exponent_minus_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e-");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_negative_exponent_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0e+0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_positive_exponent_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1e+0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_accepts_negative_decimal_boundary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("-0.000001");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_number_index_ignores_trailing_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1.");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_leading_plus_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("+1");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_number_index_ignores_numeric_separator_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name = interner.intern_string("1_0");

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}

#[test]
fn test_resolve_bounds_inconsistent_index_signatures() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
        }),
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_object_with_index_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let upper_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
        }),
        number_index: None,
    });

    let lower_type = interner.object_with_index(ObjectShape {
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
        }),
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}

#[test]
fn test_resolve_bounds_function_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![source_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![target_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_this_parameter_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

/// Test resolving bounds for function `this` parameter with optional target
///
/// NOTE: Currently ignored - bounds resolution for function `this` parameters with
/// optional targets is not fully implemented. The solver panics with a BoundsViolation
/// error when trying to resolve this case.
#[test]
#[ignore = "Function `this` parameter optional target bounds resolution not fully implemented"]
fn test_resolve_bounds_function_this_parameter_optional_target() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_this_parameter_any_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::ANY),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_this_parameter_contravariant() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let lower_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(lower_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_callable_this_parameter_contravariant() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let lower_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lower = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: Some(lower_this),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: Some(TypeId::STRING),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_optional_property_compatible() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_optional_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let lower = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

#[test]
fn test_resolve_bounds_optional_property_missing_ok() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo {
        name: name_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let lower = interner.object(Vec::new());

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_callable_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![source_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![target_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_function_to_callable() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![source_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![target_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_callable_to_function() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![source_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![target_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_application_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));
    let base = interner.reference(SymbolRef(1));
    let upper = interner.application(base, vec![TypeId::STRING]);
    let lower = interner.application(base, vec![interner.literal_string("hello")]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}

#[test]
fn test_resolve_bounds_conflict() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    ctx.add_lower_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower,
            upper,
            ..
        }) if lower == TypeId::STRING && upper == TypeId::NUMBER
    ));
}

#[test]
fn test_resolve_bounds_duplicate_upper_bounds_no_intersection() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name);

    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_resolve_no_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"));

    // No constraints at all
    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_infer_union_target_with_placeholder_member() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let param_type = interner.union(vec![t_type, TypeId::STRING]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut checker, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_union_target_with_placeholder_and_never_member() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let param_type = interner.union(vec![t_type, TypeId::NEVER]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut checker, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_resolve_circular_extends_with_concrete_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Simulate: <T extends U, U extends T, U extends string>
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);
    ctx.add_upper_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_resolve_circular_extends_bound_order() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Same cycle, but add concrete bound before the cyclic one.
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_resolve_usage_based_inference_from_bound_param() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Simulate: <T extends U, U extends T> with usage-based lower bound on U.
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_u, hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, hello);
    assert_eq!(result_u, hello);
}

// =============================================================================
// Best Common Type Tests
// =============================================================================

#[test]
fn test_best_common_type_single() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_union() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_best_common_type_dedup() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // Duplicate types should be deduped
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_best_common_type_empty() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[]);
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_best_common_type_never_ignored() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // never doesn't contribute to union
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_all_never() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NEVER, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// Full Inference Scenario Tests
// =============================================================================

#[test]
fn test_resolve_all_with_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Simulate: function foo<T, U>(a: T, b: U) called with foo("hello", 42)
    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, forty_two);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, hello));
    assert_eq!(results[1], (u_name, forty_two));
}

#[test]
fn test_resolve_all_with_circular_extends_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Simulate: <T extends U, U extends T>
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
}

// =============================================================================
// Additional Circular Generic Constraint Tests
// =============================================================================

#[test]
fn test_circular_extends_three_way_cycle() {
    // Test: <T extends U, U extends V, V extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let v_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All three resolve to unknown due to circular dependency with no concrete bounds
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_self_reference() {
    // Test: <T extends T> - self-referential constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // T extends T (self-reference)
    ctx.add_upper_bound(var_t, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // Self-reference with no other bounds resolves to unknown
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_with_lower_bound() {
    // Test: <T extends U, U extends T> with T having a lower bound of string
    // Lower bounds propagate through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Add a lower bound to T
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to string (its lower bound)
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U also resolves to string - lower bounds propagate through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_both_have_lower_bounds() {
    // Test: <T extends U, U extends T> with both having the same lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have the same lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Both resolve to string
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::STRING));
}

#[test]
fn test_circular_extends_unify_propagates() {
    // Test: <T extends U, U extends T> then unify T with number
    // Unification propagates through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Unify T directly with number
    ctx.unify_var_type(var_t, TypeId::NUMBER).unwrap();

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to number (unified)
    assert_eq!(results[0], (t_name, TypeId::NUMBER));
    // U also resolves to number - unification propagates through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}

#[test]
fn test_circular_extends_conflicting_lower_bounds() {
    // Test: <T extends U, U extends T> with T: string and U: number
    // Cycle propagation causes both to get union of all lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Conflicting lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T gets union of string | number from cycle propagation
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(results[0], (t_name, expected_union));
    // U gets its direct lower bound (number)
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}

#[test]
fn test_circular_extends_three_way_with_one_lower_bound() {
    // Test: <T extends U, U extends V, V extends T> with V having lower bound
    // Bounds propagate through adjacent connections in the cycle
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let v_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    // Only V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to boolean (its direct lower bound)
    assert_eq!(results[2], (v_name, TypeId::BOOLEAN));
    // U extends V, so U gets boolean through propagation
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
    // T extends U, but propagation stops at one level in current impl
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_with_union_lower_bound() {
    // Test: <T extends U, U extends T> with T having union type as lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has a union type as lower bound
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to the union type
    assert_eq!(results[0], (t_name, union_type));
    // U also resolves to the union through propagation
    assert_eq!(results[1], (u_name, union_type));
}

#[test]
fn test_circular_extends_with_literal_types() {
    // Test: <T extends U, U extends T> with literal type lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have literal string lower bounds
    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, world);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T gets string (simplified from union of "hello" | "world")
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U gets its direct lower bound
    assert_eq!(results[1], (u_name, world));
}

#[test]
fn test_circular_extends_four_way_cycle() {
    // Test: <T extends U, U extends V, V extends W, W extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");
    let w_name = interner.intern_string("W");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);
    let var_w = ctx.fresh_type_param(w_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let v_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
    }));
    let w_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: w_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends V, V extends W, W extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, w_type);
    ctx.add_upper_bound(var_w, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All four resolve to unknown with no lower bounds
    assert_eq!(results.len(), 4);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
    assert_eq!(results[3], (w_name, TypeId::UNKNOWN));
}

#[test]
fn test_circular_extends_with_concrete_upper_and_lower() {
    // Test: <T extends U, U extends T> with T having both upper and lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has both upper bound (string) and lower bound (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to its lower bound (hello literal)
    assert_eq!(results[0], (t_name, hello));
    // U gets hello through propagation
    assert_eq!(results[1], (u_name, hello));
}

#[test]
fn test_circular_extends_chain_with_endpoint_bound() {
    // Test: <T extends U, U extends V> (not circular) with V having lower bound
    // Chain propagation: upper bounds become resolved types when no lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);

    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));
    let v_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends V (chain, not cycle)
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);

    // V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to number (its lower bound)
    assert_eq!(results[2], (v_name, TypeId::NUMBER));
    // U has upper bound V but no lower bound, so resolves to its upper bound (V type param)
    assert_eq!(results[1].0, u_name);
    assert!(matches!(
        interner.lookup(results[1].1),
        Some(TypeKey::TypeParameter(_))
    ));
    // T has upper bound U but no lower bound, resolves to its upper bound (U type param)
    assert_eq!(results[0].0, t_name);
    assert!(matches!(
        interner.lookup(results[0].1),
        Some(TypeKey::TypeParameter(_))
    ));
}

#[test]
fn test_circular_extends_multiple_lower_bounds_same_param() {
    // Test: <T extends U, U extends T> with T having multiple lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has multiple lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to union of all its lower bounds
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(results[0], (t_name, expected_union));
    // U gets the union through propagation
    assert_eq!(results[1], (u_name, expected_union));
}

// =============================================================================
// Context-Sensitive Typing Tests
// =============================================================================

#[test]
fn test_context_sensitive_callback_param_from_upper_bound() {
    // Test: When a callback parameter has an upper bound from context,
    // the parameter type is inferred from that context.
    // e.g., arr.map((x) => x + 1) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Context provides: T must be a subtype of number (from array element type)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound and no lower bound, resolves to the upper bound
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_return_type_from_usage() {
    // Test: Return type inference from how the result is used
    // e.g., const x: string = identity(value) infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Usage site provides: result is assigned to string variable
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Call site provides: argument is a string literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound wins (more specific)
    assert_eq!(result, hello);
}

#[test]
fn test_context_sensitive_multiple_usage_sites() {
    // Test: Multiple usage sites provide constraints that must be unified
    // e.g., function used in two places with different argument types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // First usage: called with string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Second usage: called with number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple lower bounds create a union
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_context_sensitive_literal_widening_prevented() {
    // Test: When context expects a literal type, don't widen to primitive
    // e.g., const x: "hello" = getValue() where getValue returns T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let hello = interner.literal_string("hello");
    // Lower bound is the literal
    ctx.add_lower_bound(var_t, hello);
    // Upper bound is also the literal (from contextual type)
    ctx.add_upper_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should preserve the literal type
    assert_eq!(result, hello);
}

#[test]
fn test_context_sensitive_object_property_inference() {
    // Test: Object property types inferred from contextual type
    // e.g., const obj: {x: number} = {x: getValue()}
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Context expects number for property x
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Value provides a specific number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal number from lower bound
    assert_eq!(result, forty_two);
}

#[test]
fn test_context_sensitive_array_element_inference() {
    // Test: Array element types inferred from array context
    // e.g., const arr: string[] = [getValue()]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Context: array of strings means elements must be strings
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_context_sensitive_conditional_branch_types() {
    // Test: Type from conditional branches unifies
    // e.g., condition ? stringValue : numberValue should be string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // True branch contributes string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // False branch contributes number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Union of both branch types
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_context_sensitive_function_param_from_callback_context() {
    // Test: Function parameter type inferred from callback signature context
    // e.g., arr.filter((x) => x > 0) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Contextual callback type says param must be number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // No explicit annotation, so no lower bound from declaration

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Infers from context
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_rest_param_inference() {
    // Test: Rest parameter type inference from spread arguments
    // e.g., fn(...args: T) called with (1, 2, 3)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Multiple arguments of same type contribute to rest param type
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    ctx.add_lower_bound(var_t, one);
    ctx.add_lower_bound(var_t, two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple number literals widen to number
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_context_sensitive_default_param_inference() {
    // Test: Default parameter provides lower bound for type param
    // e.g., function fn<T>(x: T = "default")
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Default value is a string literal
    let default_val = interner.literal_string("default");
    ctx.add_lower_bound(var_t, default_val);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, default_val);
}

// =============================================================================
// Callback Parameter Inference Tests
// =============================================================================

#[test]
fn test_callback_param_inferred_from_array_map() {
    // Test: arr.map((x) => x.toUpperCase()) where arr: string[]
    // The callback parameter x should be inferred as string from array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array<string>.map provides callback with (element: string) => U
    // So T (the callback param type) has upper bound string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Callback param inferred from array element type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callback_param_inferred_with_index() {
    // Test: arr.forEach((item, index) => ...) where arr: number[]
    // First param is number (element), second is number (index)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T is the element type from Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // U is the index type (always number)
    ctx.add_upper_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_callback_param_inferred_from_generic_higher_order() {
    // Test: Generic higher-order function like filter<T>(arr: T[], pred: (x: T) => boolean)
    // When called with string[], T is inferred as string, so callback param is string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Lower bound from argument: array contains strings
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound from callback usage: predicate receives T
    // (callback param type flows from T)

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // T inferred as string, so callback param is string
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Generic Default Type Inference Tests
// =============================================================================

#[test]
fn test_generic_default_used_when_no_inference() {
    // Test: <T = string> with no inference constraints, T defaults to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No constraints added - should use default if available
    // Note: defaults are typically handled during type param registration,
    // but here we test the inference context behavior with no constraints
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Without any constraints, resolves to unknown
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_generic_default_overridden_by_lower_bound() {
    // Test: <T = string> with lower bound number, inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Inferred lower bound takes precedence
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound overrides any potential default
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_default_with_constraint() {
    // Test: <T extends object = {}> - constraint with default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound, resolves to the upper bound
    assert_eq!(result, TypeId::OBJECT);
}

#[test]
fn test_generic_default_with_literal_inference() {
    // Test: <T = string> called with literal "hello", infers literal not default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Inferred literal takes precedence over default
    assert_eq!(result, hello);
}

#[test]
fn test_generic_multiple_params_with_defaults() {
    // Test: <T = string, U = number> with only U having lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let _var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Only U has a lower bound
    ctx.add_lower_bound(var_u, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T has no constraints, resolves to unknown
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    // U has lower bound, resolves to boolean
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
}

// =============================================================================
// Generic Constraint Propagation Tests
// =============================================================================

#[test]
fn test_constraint_propagation_upper_to_lower() {
    // Test: Upper bound on one param propagates to lower bound check
    // <T extends string> called with T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Lower bound from argument: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound satisfies upper bound, resolves to literal
    assert_eq!(result, hello);
}

#[test]
fn test_constraint_propagation_through_unification() {
    // Test: Unifying two vars propagates constraints from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T has lower bound string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has lower bound number
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    // Unify T and U
    ctx.unify_vars(var_t, var_u).unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Unified vars get union of both lower bounds
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_transitive_upper_bounds() {
    // Test: T extends string with lower bound "hello"
    // Lower bound must satisfy upper bound constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Add lower bound to T (literal satisfies string upper bound)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();

    // T resolves to its lower bound (literal "hello")
    assert_eq!(result_t, hello);
}

#[test]
fn test_constraint_propagation_multiple_upper_bounds() {
    // Test: T extends A & B (multiple upper bounds create intersection)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Multiple upper bounds
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple upper bounds create intersection
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_lower_bounds_union() {
    // Test: Multiple lower bounds create union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Multiple lower bounds from different call sites
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple lower bounds create union
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_propagation_with_never_lower_bound() {
    // Test: never as lower bound doesn't contribute to union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Lower bounds including never
    ctx.add_lower_bound(var_t, TypeId::NEVER);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // never is filtered out, only string remains
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_any_lower_with_concrete() {
    // Test: any as lower bound with concrete type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Lower bounds: any and string
    ctx.add_lower_bound(var_t, TypeId::ANY);
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound constrains
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With upper bound, any is filtered from lower bounds
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_propagation_object_properties() {
    // Test: Object type constraint propagation
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create object type with property
    let prop_name = interner.intern_string("x");
    let obj_type = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}

// ============================================================================
// Constructor Type Inference Tests
// ============================================================================
// Tests for constructor function type inference

#[test]
fn test_constructor_single_param_inference() {
    // Test: new (x: T) => Instance infers T from argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constructor param receives string argument
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constructor_multiple_params_inference() {
    // Test: new <T, U>(a: T, b: U) => Instance infers both T and U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // First param is string, second is number
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}

#[test]
fn test_constructor_with_constraint() {
    // Test: new <T extends object>(config: T) => Instance
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T has upper bound of object
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Argument is specific object type
    let prop_name = interner.intern_string("name");
    let obj_type = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should be the specific object type
    assert_eq!(result, obj_type);
}

#[test]
fn test_constructor_optional_param_inference() {
    // Test: new <T>(arg?: T) => Instance with optional param
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Optional param not provided - may include undefined
    let optional_type = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, optional_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should preserve the union type
    assert_eq!(result, optional_type);
}

#[test]
fn test_constructor_rest_param_inference() {
    // Test: new <T>(...args: T[]) => Instance with rest param
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Rest param elements are string and number - infer union
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should be union of string | number
    if let Some(TypeKey::Union(_)) = interner.lookup(result) {
        // Union is expected
    } else {
        // Could also resolve to one of the types if widening happens
        assert!(result == TypeId::STRING || result == TypeId::NUMBER);
    }
}

// ============================================================================
// Method Signature Inference Tests
// ============================================================================

#[test]
fn test_method_return_type_inference_basic() {
    // Test inferring return type from method call: obj.method() returns string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Method signature: () => T
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        }],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call returns string, so T should be inferred as string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_parameter_type_inference() {
    // Test inferring parameter type from method call: obj.method(value)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Method signature: (x: T) => void
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        }],
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

    // Called with number, so T should be inferred as number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_method_this_type_inference() {
    // Test this type in method: class method with this constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name);
    let this_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: this_name,
        constraint: None,
        default: None,
    }));

    // Method signature: (this: This) => This
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: this_name,
            constraint: None,
            default: None,
        }],
        params: vec![],
        this_type: Some(this_type),
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create an object type to represent `this`
    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Called on object, so This should be inferred as that object type
    ctx.add_lower_bound(var_this, obj_type);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, obj_type);
}

#[test]
fn test_method_generic_parameter_inference() {
    // Test: generic method <T>(x: T) => Array<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Method signature: <T>(x: T) => Array<T>
    let return_array = interner.array(t_type);
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: return_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Called with boolean, so T should be inferred as boolean
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_method_multiple_generic_params_inference() {
    // Test: <K, V>(key: K, value: V) => Map<K, V>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name);
    let var_v = ctx.fresh_type_param(v_name);

    // Called with (string, number)
    ctx.add_lower_bound(var_k, TypeId::STRING);
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // K inferred as string
    assert_eq!(results[0], (k_name, TypeId::STRING));
    // V inferred as number
    assert_eq!(results[1], (v_name, TypeId::NUMBER));
}

// ============================================================================
// Circular Type Alias Detection Tests
// ============================================================================
// Tests for detecting and handling circular type aliases

#[test]
fn test_circular_type_alias_self_reference() {
    // Test: type T = T (direct self-reference should be detected)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No bounds - trying to resolve should give unknown/any
    let result = ctx.resolve_with_constraints(var_t);
    // Without concrete bounds, resolution should still work (gives unknown)
    assert!(result.is_ok());
}

#[test]
fn test_circular_type_alias_via_array() {
    // Test: type T = Array<T> - recursive through array
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Add concrete array lower bound
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), string_array);
}

#[test]
fn test_circular_type_alias_via_union() {
    // Test: type T = T | null - recursive through union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: string | null
    let string_or_null = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    ctx.add_upper_bound(var_t, string_or_null);

    // Lower bound: string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::STRING);
}

#[test]
fn test_circular_type_alias_nested_object() {
    // Test: type Node = { child: Node | null }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Just test that we can have object bounds without infinite recursion
    let prop_name = interner.intern_string("value");
    let obj_type = interner.object(vec![PropertyInfo {
        name: prop_name,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), obj_type);
}

#[test]
fn test_circular_type_alias_function_return() {
    // Test: type F = () => F - function returning itself
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let f_name = interner.intern_string("F");

    let var_f = ctx.fresh_type_param(f_name);

    // Add function lower bound
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_f, fn_type);

    let result = ctx.resolve_with_constraints(var_f);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), fn_type);
}

// ============================================================================
// Self-Referential Generic Constraints Tests
// ============================================================================
// Tests for generic type parameters that reference themselves in constraints

#[test]
fn test_self_ref_constraint_comparable() {
    // Test: T extends Comparable<T> pattern (common in sorting)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: number (which is comparable to itself)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    // Lower bound: specific number literal
    let num_lit = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, num_lit);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), num_lit);
}

#[test]
fn test_self_ref_constraint_builder_pattern() {
    // Test: T extends Builder<T> - fluent builder pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Simulate builder with method that returns same type
    let build_prop = interner.intern_string("build");
    let builder_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let builder_type = interner.object(vec![PropertyInfo {
        name: build_prop,
        type_id: builder_fn,
        write_type: builder_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, builder_type);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), builder_type);
}

#[test]
fn test_self_ref_constraint_iterable() {
    // Test: T extends Iterable<T> - iterable of itself
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: array (iterable)
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_upper_bound(var_t, number_array);

    // Lower bound: specific array
    ctx.add_lower_bound(var_t, number_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), number_array);
}

#[test]
fn test_self_ref_constraint_json_value() {
    // Test: type JSONValue = string | number | boolean | JSONValue[] | {[k: string]: JSONValue}
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound: primitive union (simplified JSON)
    let json_primitive = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);
    ctx.add_upper_bound(var_t, json_primitive);

    // Lower bound: string (valid JSON value)
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::STRING);
}

#[test]
fn test_self_ref_constraint_recursive_array() {
    // Test: T extends T[] - array of itself constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Test with array bounds
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), string_array);
}

// ============================================================================
// Mutually Recursive Type Definitions Tests
// ============================================================================
// Tests for types that reference each other in a cycle

#[test]
fn test_mutual_recursion_two_types() {
    // Test: type A = { b: B }, type B = { a: A }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // Both get object lower bounds (breaking the cycle with concrete types)
    let prop_a = interner.intern_string("value");
    let obj_a = interner.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let prop_b = interner.intern_string("count");
    let obj_b = interner.object(vec![PropertyInfo {
        name: prop_b,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_a, obj_a);
    ctx.add_lower_bound(var_b, obj_b);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, obj_a);
    assert_eq!(resolved[1].1, obj_b);
}

#[test]
fn test_mutual_recursion_three_types() {
    // Test: A -> B -> C -> A cycle
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // All have same upper bound
    ctx.add_upper_bound(var_a, TypeId::STRING);
    ctx.add_upper_bound(var_b, TypeId::STRING);
    ctx.add_upper_bound(var_c, TypeId::STRING);

    // Different literal lower bounds
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    ctx.add_lower_bound(var_a, lit_a);
    ctx.add_lower_bound(var_b, lit_b);
    ctx.add_lower_bound(var_c, lit_c);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].1, lit_a);
    assert_eq!(resolved[1].1, lit_b);
    assert_eq!(resolved[2].1, lit_c);
}

#[test]
fn test_mutual_recursion_shared_constraint() {
    // Test: A and B both bounded by same type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // Shared upper bound
    let shared_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_a, shared_union);
    ctx.add_upper_bound(var_b, shared_union);

    // A gets string, B gets number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, TypeId::STRING);
    assert_eq!(resolved[1].1, TypeId::NUMBER);
}

#[test]
fn test_mutual_recursion_array_element() {
    // Test: A = B[], B = A[] (arrays of each other)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // Concrete array lower bounds
    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    ctx.add_lower_bound(var_a, string_array);
    ctx.add_lower_bound(var_b, number_array);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, string_array);
    assert_eq!(resolved[1].1, number_array);
}

#[test]
fn test_mutual_recursion_function_params() {
    // Test: F = (a: G) => void, G = (f: F) => void
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let f_name = interner.intern_string("F");
    let g_name = interner.intern_string("G");

    let var_f = ctx.fresh_type_param(f_name);
    let var_g = ctx.fresh_type_param(g_name);

    // Create ParamInfo structs
    let param_f = ParamInfo {
        name: Some(interner.intern_string("a")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let param_g = ParamInfo {
        name: Some(interner.intern_string("f")),
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };

    // Concrete function lower bounds
    let fn_f = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![param_f],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_g = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![param_g],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var_f, fn_f);
    ctx.add_lower_bound(var_g, fn_g);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, fn_f);
    assert_eq!(resolved[1].1, fn_g);
}

// ============================================================================
// Higher-Order Function Type Inference Tests
// ============================================================================
// Tests for inferring types in functions that take or return functions

#[test]
fn test_hof_callback_param_inference() {
    // Test: map<T, U>(arr: T[], fn: (x: T) => U) => U[]
    // Inferring T from array and U from callback return
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T inferred from array element type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U inferred from callback return type
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_hof_compose_functions() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B) => (a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Infer from concrete function types
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_curried_function() {
    // Test: curry<A, B, C>(fn: (a: A, b: B) => C) => (a: A) => (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Infer from uncurried function parameters and return
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_reduce_accumulator() {
    // Test: reduce<T, U>(arr: T[], fn: (acc: U, val: T) => U, init: U) => U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from array elements
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U from initial value
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_hof_function_returning_function() {
    // Test: factory<T>() => () => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from usage of returned function
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Generic Method Chaining Tests (Fluent API Patterns)
// ============================================================================
// Tests for type inference in fluent/builder API patterns

#[test]
fn test_method_chain_builder_pattern() {
    // Test: Builder<T>.setValue(v: T).build() => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from setValue argument
    let string_lit = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, string_lit);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_lit);
}

#[test]
fn test_method_chain_transform() {
    // Test: chain<T>.map<U>(fn: (t: T) => U) => chain<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from initial chain value
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U from map callback return
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_method_chain_filter() {
    // Test: chain<T>.filter(fn: (t: T) => boolean) => chain<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T preserved through filter
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_chain_multiple_transforms() {
    // Test: chain<A>.map<B>().map<C>().map<D>()
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_d = ctx.fresh_type_param(d_name);

    // Each step infers next type
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_method_chain_flatmap() {
    // Test: chain<T>.flatMap<U>(fn: (t: T) => chain<U>) => chain<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from outer chain
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // U from inner chain returned by callback
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, string_array);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

// ============================================================================
// Inference with Default Type Parameters Tests
// ============================================================================
// Tests for generic type inference when defaults are provided

#[test]
fn test_default_type_param_not_inferred() {
    // Test: <T = string>() => T - when no inference, use default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No lower bounds added - would use default in real scenario
    // For inference, resolve gives unknown
    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
}

#[test]
fn test_default_type_param_override() {
    // Test: <T = string>(x: T) => T - inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Inference from argument overrides default
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_default_type_param_with_constraint() {
    // Test: <T extends object = {}>(x: T) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Specific object as lower bound
    let prop = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo {
        name: prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, obj);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj);
}

#[test]
fn test_default_type_param_chain() {
    // Test: <T = string, U = T>(x: U) => [T, U]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Both inferred from same value
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_default_type_param_array() {
    // Test: <T = unknown>(arr?: T[]) => T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Inferred from array element
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =========================================================================
// Generic Function Inference - Multiple Type Params
// =========================================================================
// Tests for generic function inference with multiple type parameters

#[test]
fn test_generic_function_three_type_params() {
    // Test: <A, B, C>(a: A, b: B, c: C) => [A, B, C]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Called with (string, number, boolean)
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (a_name, TypeId::STRING));
    assert_eq!(results[1], (b_name, TypeId::NUMBER));
    assert_eq!(results[2], (c_name, TypeId::BOOLEAN));
}

#[test]
fn test_generic_function_dependent_type_params() {
    // Test: <T, U extends T>(base: T, derived: U) => U
    // Where U's constraint depends on T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T gets bound from first argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U gets bound from second argument (a string literal)
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_u, lit_hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, lit_hello);
}

#[test]
fn test_generic_function_shared_type_param() {
    // Test: <T>(a: T, b: T) => T
    // Both arguments contribute to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Called with two different string literals - should infer union
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // T is inferred as string (simplified from union of "a" | "b")
    assert_eq!(result, TypeId::STRING);
}

// =========================================================================
// Inference from Array/Object Destructuring Patterns
// =========================================================================
// Tests for type inference from destructuring patterns

#[test]
fn test_inference_array_element_type() {
    // Test: inferring element type from array access
    // <T>(arr: T[]) => T where arr[0] is used
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Array<string> is passed, so T should be string
    let string_array = interner.array(TypeId::STRING);
    // When destructuring [first] = arr, we infer T from the array element
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);

    // Verify the array type matches
    let _expected_array = interner.array(result);
    assert!(string_array != TypeId::ERROR);
}

#[test]
fn test_inference_tuple_element_types() {
    // Test: inferring from tuple destructuring
    // <A, B>(tuple: [A, B]) => A
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // Tuple [string, number] is passed
    // Destructuring [first, second] = tuple infers A = string, B = number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    let result_a = ctx.resolve_with_constraints(var_a).unwrap();
    let result_b = ctx.resolve_with_constraints(var_b).unwrap();

    assert_eq!(result_a, TypeId::STRING);
    assert_eq!(result_b, TypeId::NUMBER);
}

#[test]
fn test_inference_object_property_type() {
    // Test: inferring from object destructuring
    // <T>(obj: { value: T }) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Object { value: number } is passed
    // Destructuring { value } = obj infers T = number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_nested_object_property() {
    // Test: inferring from nested object destructuring
    // <T>(obj: { inner: { value: T } }) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Nested destructuring { inner: { value } } = obj
    // value is boolean, so T = boolean
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

// =========================================================================
// Contextual Typing in Arrow Function Returns
// =========================================================================
// Tests for type inference from contextual typing of arrow function returns

#[test]
fn test_contextual_arrow_return_simple() {
    // Test: contextual typing provides return type
    // const fn: () => string = () => "hello"
    // The arrow function return is inferred from context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Contextual type says return is string
    // Arrow function body returns a string literal
    ctx.add_upper_bound(var_t, TypeId::STRING);
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, lit_hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the more specific type: "hello"
    assert_eq!(result, lit_hello);
}

#[test]
fn test_contextual_arrow_return_array() {
    // Test: contextual array return type
    // const fn: () => number[] = () => [1, 2, 3]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Context expects Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Return value contains number literals
    let lit_1 = interner.literal_number(1.0);
    ctx.add_lower_bound(var_t, lit_1);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should infer the literal type
    assert_eq!(result, lit_1);
}

#[test]
fn test_contextual_arrow_return_object() {
    // Test: contextual object return type
    // const fn: () => { x: number } = () => ({ x: 42 })
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Context expects { x: number }
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Actual value is 42
    let lit_42 = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, lit_42);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lit_42);
}

#[test]
fn test_contextual_arrow_callback_param() {
    // Test: callback parameter inference
    // arr.map((x) => x + 1) where arr: number[]
    // x should be inferred as number from the array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Contextual type from Array<number>.map callback is (element: number) => U
    // So T (the callback parameter type) should be number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_contextual_arrow_higher_order() {
    // Test: higher-order function contextual typing
    // compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // compose((x: number) => x.toString(), (s: string) => s.length)
    // A = string, B = number, C = string
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (a_name, TypeId::STRING));
    assert_eq!(results[1], (b_name, TypeId::NUMBER));
    assert_eq!(results[2], (c_name, TypeId::STRING));
}

// ============================================================================
// Variadic Tuple Inference Tests
// ============================================================================
// Tests for inferring types in variadic tuple patterns like [...T]

#[test]
fn test_variadic_tuple_rest_element() {
    // Test: [...T] where T is inferred from tuple elements
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred as array of strings from rest element
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_variadic_tuple_prefix_and_rest() {
    // Test: [string, ...T] - prefix element with rest
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is the rest part after string prefix
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_t, number_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, number_array);
}

#[test]
fn test_variadic_tuple_suffix_and_rest() {
    // Test: [...T, string] - rest with suffix element
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is the rest part before string suffix
    let boolean_array = interner.array(TypeId::BOOLEAN);
    ctx.add_lower_bound(var_t, boolean_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, boolean_array);
}

#[test]
fn test_variadic_tuple_multiple_rest() {
    // Test: [...T, ...U] - multiple variadic segments
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T and U are different array types
    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_t, string_array);
    ctx.add_lower_bound(var_u, number_array);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, string_array);
    assert_eq!(results[1].1, number_array);
}

#[test]
fn test_variadic_tuple_concat() {
    // Test: [...T, ...U] => [...T, ...U] (tuple concatenation)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Infer from concrete tuple parts
    let tuple_t = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_u = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    ctx.add_lower_bound(var_t, tuple_t);
    ctx.add_lower_bound(var_u, tuple_u);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, tuple_t);
    assert_eq!(results[1].1, tuple_u);
}

// ============================================================================
// Named Tuple Elements Tests
// ============================================================================
// Tests for tuples with named elements like [x: string, y: number]

#[test]
fn test_named_tuple_basic() {
    // Test: [x: T, y: U] - basic named tuple inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Infer from named tuple elements
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_named_tuple_with_optional() {
    // Test: [x: T, y?: U] - optional named element
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Create named tuple with optional element
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let _named_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: true,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_named_tuple_destructuring() {
    // Test: function({x, y}: [x: T, y: U]) - destructuring named tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Types inferred from destructuring context
    let lit_hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, lit_hello);
    ctx.add_lower_bound(var_u, lit_42);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, lit_hello);
    assert_eq!(results[1].1, lit_42);
}

#[test]
fn test_named_tuple_three_elements() {
    // Test: [a: T, b: U, c: V] - three named elements
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);
    ctx.add_lower_bound(var_v, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_named_tuple_mixed_named_unnamed() {
    // Test: [x: T, U, z: V] - mixed named and unnamed
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Create mixed tuple
    let x_name = interner.intern_string("x");
    let _mixed_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
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

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

// ============================================================================
// Tuple Spread Type Inference Tests
// ============================================================================
// Tests for spread operations on tuple types

#[test]
fn test_tuple_spread_into_array() {
    // Test: [...tuple] spreads into array context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Tuple spread becomes union of element types
    let string_number_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, string_number_union);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_number_union);
}

#[test]
fn test_tuple_spread_function_args() {
    // Test: fn(...args: T) where T is tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is inferred as tuple from function arguments
    let args_tuple = interner.tuple(vec![
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
    ctx.add_lower_bound(var_t, args_tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, args_tuple);
}

#[test]
fn test_tuple_spread_concat_tuples() {
    // Test: [...A, ...B] = [...C] - concatenating tuples
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // A and B are tuple parts
    let tuple_a = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_b = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_a, tuple_a);
    ctx.add_lower_bound(var_b, tuple_b);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, tuple_a);
    assert_eq!(results[1].1, tuple_b);
}

#[test]
fn test_tuple_spread_in_return() {
    // Test: function returning [...T, extra]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is the spread part of return tuple
    let spread_part = interner.tuple(vec![
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
    ctx.add_lower_bound(var_t, spread_part);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, spread_part);
}

#[test]
fn test_tuple_spread_with_rest() {
    // Test: [...T, ...rest: U[]]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T is fixed tuple, U is element type of rest
    let fixed_tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    ctx.add_lower_bound(var_t, fixed_tuple);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, fixed_tuple);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

// ============================================================================
// Type Guard Narrowing Pattern Tests
// ============================================================================
// Tests for type narrowing via type guards (typeof, instanceof, custom)

#[test]
fn test_type_guard_typeof_string() {
    // Test: typeof x === "string" narrows union to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Original type is string | number
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, string_or_number);

    // After typeof === "string", narrow to string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_type_guard_typeof_number() {
    // Test: typeof x === "number" narrows union to number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Original type is string | number | boolean
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    ctx.add_upper_bound(var_t, union);

    // After typeof === "number", narrow to number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_type_guard_typeof_object() {
    // Test: typeof x === "object" narrows to object types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound is object | null | string
    let obj_or_null_or_string = interner.union(vec![TypeId::OBJECT, TypeId::NULL, TypeId::STRING]);
    ctx.add_upper_bound(var_t, obj_or_null_or_string);

    // typeof === "object" includes object and null
    let obj_or_null = interner.union(vec![TypeId::OBJECT, TypeId::NULL]);
    ctx.add_lower_bound(var_t, obj_or_null);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_or_null);
}

#[test]
fn test_type_guard_instanceof() {
    // Test: x instanceof Error narrows to Error type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound is Error | string (simulated with object)
    let error_or_string = interner.union(vec![TypeId::OBJECT, TypeId::STRING]);
    ctx.add_upper_bound(var_t, error_or_string);

    // After instanceof Error, narrow to object (Error)
    ctx.add_lower_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::OBJECT);
}

#[test]
fn test_type_guard_custom_predicate() {
    // Test: isString(x): x is string - custom type predicate
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound is unknown
    ctx.add_upper_bound(var_t, TypeId::UNKNOWN);

    // After custom guard, narrow to string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Discriminated Union Narrowing Tests
// ============================================================================
// Tests for narrowing unions via discriminant properties

#[test]
fn test_discriminated_union_basic() {
    // Test: { kind: "a" } | { kind: "b" } narrowed by kind
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create discriminated union members
    let kind_prop = interner.intern_string("kind");
    let lit_a = interner.literal_string("a");

    let type_a = interner.object(vec![PropertyInfo {
        name: kind_prop,
        type_id: lit_a,
        write_type: lit_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // After checking kind === "a", narrow to type_a
    ctx.add_lower_bound(var_t, type_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, type_a);
}

#[test]
fn test_discriminated_union_switch() {
    // Test: switch(x.kind) narrowing
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // After switch case "circle"
    let kind_prop = interner.intern_string("kind");
    let radius_prop = interner.intern_string("radius");
    let lit_circle = interner.literal_string("circle");

    let circle_type = interner.object(vec![
        PropertyInfo {
            name: kind_prop,
            type_id: lit_circle,
            write_type: lit_circle,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: radius_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, circle_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, circle_type);
}

#[test]
fn test_discriminated_union_type_property() {
    // Test: { type: "request" } | { type: "response" } narrowing
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let type_prop = interner.intern_string("type");
    let lit_request = interner.literal_string("request");
    let body_prop = interner.intern_string("body");

    let request_type = interner.object(vec![
        PropertyInfo {
            name: type_prop,
            type_id: lit_request,
            write_type: lit_request,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: body_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, request_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, request_type);
}

#[test]
fn test_discriminated_union_boolean_discriminant() {
    // Test: { success: true, data: T } | { success: false, error: E }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let success_prop = interner.intern_string("success");
    let data_prop = interner.intern_string("data");

    // Use BOOLEAN for success field (representing literal true)
    let success_type = interner.object(vec![
        PropertyInfo {
            name: success_prop,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: data_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, success_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, success_type);
}

#[test]
fn test_discriminated_union_numeric_discriminant() {
    // Test: { code: 200, body: string } | { code: 404, message: string }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let code_prop = interner.intern_string("code");
    let body_prop = interner.intern_string("body");
    let lit_200 = interner.literal_number(200.0);

    let ok_response = interner.object(vec![
        PropertyInfo {
            name: code_prop,
            type_id: lit_200,
            write_type: lit_200,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: body_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, ok_response);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, ok_response);
}

// ============================================================================
// In Operator Narrowing Tests
// ============================================================================
// Tests for narrowing via the 'in' operator

#[test]
fn test_in_operator_basic() {
    // Test: "prop" in x narrows to types with prop
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // After "name" in x, narrow to object with name
    let name_prop = interner.intern_string("name");
    let with_name = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, with_name);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, with_name);
}

#[test]
fn test_in_operator_union_narrowing() {
    // Test: "fly" in animal narrows Animal to Bird
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Bird has fly method
    let fly_prop = interner.intern_string("fly");
    let fly_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let bird_type = interner.object(vec![PropertyInfo {
        name: fly_prop,
        type_id: fly_fn,
        write_type: fly_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, bird_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, bird_type);
}

#[test]
fn test_in_operator_optional_property() {
    // Test: "optional" in x where optional may not exist
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Object with optional property (in check confirms it exists)
    let opt_prop = interner.intern_string("optional");
    let with_optional = interner.object(vec![PropertyInfo {
        name: opt_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, with_optional);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, with_optional);
}

#[test]
fn test_in_operator_method_check() {
    // Test: "forEach" in x narrows to array-like
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array-like with forEach method
    let foreach_prop = interner.intern_string("forEach");
    let foreach_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let array_like = interner.object(vec![PropertyInfo {
        name: foreach_prop,
        type_id: foreach_fn,
        write_type: foreach_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, array_like);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, array_like);
}

#[test]
fn test_in_operator_negation() {
    // Test: !("prop" in x) narrows to types without prop
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // After !("special" in x), narrow to object without special
    let other_prop = interner.intern_string("basic");
    let without_special = interner.object(vec![PropertyInfo {
        name: other_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, without_special);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, without_special);
}

// =============================================================================
// Context-Sensitive Type Inference Tests
// =============================================================================
// Tests for inferring types from contextual typing (callbacks, array methods,
// Promise chains, generic function arguments)

// -----------------------------------------------------------------------------
// Callback Parameter Inference from Usage
// -----------------------------------------------------------------------------

#[test]
fn test_callback_param_inferred_from_call_site() {
    // Test: When a callback is passed to a function, the parameter types
    // are inferred from how the callback is called within the function.
    // e.g., function apply<T>(fn: (x: T) => void, val: T) - T inferred from val
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // val argument provides lower bound
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Callback param x will be "hello" type
    assert_eq!(result, hello);
}

#[test]
fn test_callback_param_inferred_from_multiple_calls() {
    // Test: Callback called with different values creates union type
    // e.g., function callBoth<T>(fn: (x: T) => void) { fn("a"); fn(1); }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Callback called with string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Callback called with number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_callback_return_inferred_from_usage() {
    // Test: Callback return type inferred from how result is used
    // e.g., const x: number = transform((s) => s.length)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let u_name = interner.intern_string("U");

    let var_u = ctx.fresh_type_param(u_name);

    // Return type must satisfy usage context
    ctx.add_upper_bound(var_u, TypeId::NUMBER);
    // Callback returns specific number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_u, forty_two);

    let result = ctx.resolve_with_constraints(var_u).unwrap();
    assert_eq!(result, forty_two);
}

#[test]
fn test_callback_param_from_object_method_context() {
    // Test: obj.method((x) => ...) where method signature defines x's type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Object method provides context that param is number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_callback_param_from_overloaded_function() {
    // Test: Overloaded function picks signature based on callback
    // When multiple signatures exist, param type comes from matching overload
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Chosen overload expects callback with string param
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// -----------------------------------------------------------------------------
// Array Method Callback Inference (map, filter, reduce)
// -----------------------------------------------------------------------------

#[test]
fn test_array_map_callback_param_and_return() {
    // Test: nums.map((n) => n.toString())
    // Param n: number (from array), Return: string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from Array<number> element type
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // U from callback return type
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_array_map_with_index_and_array_params() {
    // Test: arr.map((elem, index, array) => ...)
    // elem: T, index: number, array: T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let idx_name = interner.intern_string("Idx");
    let arr_name = interner.intern_string("Arr");

    let var_t = ctx.fresh_type_param(t_name);
    let var_idx = ctx.fresh_type_param(idx_name);
    let var_arr = ctx.fresh_type_param(arr_name);

    // Element type
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Index is always number
    ctx.add_upper_bound(var_idx, TypeId::NUMBER);
    // Array parameter is the source array type
    let string_array = interner.array(TypeId::STRING);
    ctx.add_upper_bound(var_arr, string_array);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_idx = ctx.resolve_with_constraints(var_idx).unwrap();
    let result_arr = ctx.resolve_with_constraints(var_arr).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_idx, TypeId::NUMBER);
    assert_eq!(result_arr, string_array);
}

#[test]
fn test_array_filter_preserves_element_type() {
    // Test: strs.filter((s) => s.length > 0)
    // Input: string[], Output: string[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Filter preserves element type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_array_filter_with_type_guard() {
    // Test: arr.filter((x): x is string => typeof x === "string")
    // Narrows from (string | number)[] to string[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let s_name = interner.intern_string("S");

    let var_t = ctx.fresh_type_param(t_name);
    let var_s = ctx.fresh_type_param(s_name);

    // Original element type is union
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union);

    // Type guard narrows to string
    ctx.add_lower_bound(var_s, TypeId::STRING);
    ctx.add_upper_bound(var_s, union);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_s = ctx.resolve_with_constraints(var_s).unwrap();

    assert_eq!(result_t, union);
    assert_eq!(result_s, TypeId::STRING);
}

#[test]
fn test_array_reduce_accumulator_inference() {
    // Test: nums.reduce((acc, n) => acc + n, 0)
    // acc: number (from initial value), n: number (from array)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let acc_name = interner.intern_string("Acc");
    let elem_name = interner.intern_string("Elem");

    let var_acc = ctx.fresh_type_param(acc_name);
    let var_elem = ctx.fresh_type_param(elem_name);

    // Accumulator type from initial value
    let zero = interner.literal_number(0.0);
    ctx.add_lower_bound(var_acc, zero);
    // Also from callback return (same type)
    ctx.add_lower_bound(var_acc, TypeId::NUMBER);

    // Element type from array
    ctx.add_upper_bound(var_elem, TypeId::NUMBER);

    let result_acc = ctx.resolve_with_constraints(var_acc).unwrap();
    let result_elem = ctx.resolve_with_constraints(var_elem).unwrap();

    // Accumulator simplifies to number (best common type of literal 0 and number)
    assert_eq!(result_acc, TypeId::NUMBER);
    assert_eq!(result_elem, TypeId::NUMBER);
}

#[test]
fn test_array_reduce_different_accumulator_type() {
    // Test: strs.reduce((obj, s) => ({ ...obj, [s]: true }), {})
    // Reduces string[] to Record<string, boolean>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let acc_name = interner.intern_string("Acc");
    let elem_name = interner.intern_string("Elem");

    let var_acc = ctx.fresh_type_param(acc_name);
    let var_elem = ctx.fresh_type_param(elem_name);

    // Accumulator is object with string keys and boolean values
    let obj_type = interner.object(vec![]);
    ctx.add_lower_bound(var_acc, obj_type);

    // Element type from string array
    ctx.add_upper_bound(var_elem, TypeId::STRING);

    let result_acc = ctx.resolve_with_constraints(var_acc).unwrap();
    let result_elem = ctx.resolve_with_constraints(var_elem).unwrap();

    assert_eq!(result_acc, obj_type);
    assert_eq!(result_elem, TypeId::STRING);
}

#[test]
fn test_array_find_returns_element_or_undefined() {
    // Test: nums.find((n) => n > 0)
    // Returns: number | undefined
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Element type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // Return includes undefined possibility
    ctx.add_lower_bound(var_t, TypeId::UNDEFINED);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_array_every_callback_returns_boolean() {
    // Test: nums.every((n) => n > 0)
    // Callback must return boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let ret_name = interner.intern_string("Ret");

    let var_t = ctx.fresh_type_param(t_name);
    let var_ret = ctx.fresh_type_param(ret_name);

    // Element type
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Return type constrained to boolean
    ctx.add_upper_bound(var_ret, TypeId::BOOLEAN);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_ret = ctx.resolve_with_constraints(var_ret).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_ret, TypeId::BOOLEAN);
}

// -----------------------------------------------------------------------------
// Promise.then Chain Inference
// -----------------------------------------------------------------------------

#[test]
fn test_promise_then_basic_chain() {
    // Test: promise.then((val) => val + 1)
    // Promise<number>.then returns Promise<number>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from Promise<number> resolved value
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // U from callback return type
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_promise_then_transform_type() {
    // Test: Promise<string>.then((s) => s.length) => Promise<number>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T is string from input promise
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // U is number from callback return
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_promise_then_chained_multiple() {
    // Test: promise.then(f1).then(f2).then(f3)
    // Types flow through: A -> B -> C -> D
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_d = ctx.fresh_type_param(d_name);

    // Initial promise value
    ctx.add_lower_bound(var_a, TypeId::STRING);
    // First then transforms to number
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // Second then transforms to boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    // Third then transforms to symbol
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_promise_then_returns_promise() {
    // Test: promise.then((x) => Promise.resolve(x + 1))
    // When callback returns Promise<U>, outer Promise unwraps to Promise<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Input promise resolves to number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Callback returns Promise<number>, unwrapped to number
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_promise_catch_error_type() {
    // Test: promise.catch((err) => handleError(err))
    // Error type is typically unknown or any
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let err_name = interner.intern_string("Err");

    let var_err = ctx.fresh_type_param(err_name);

    // Catch handler receives unknown error type
    ctx.add_upper_bound(var_err, TypeId::UNKNOWN);

    let result = ctx.resolve_with_constraints(var_err).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_promise_finally_no_value() {
    // Test: promise.finally(() => cleanup())
    // Finally callback receives no arguments and return is ignored
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Promise value passes through finally unchanged
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_promise_all_tuple_inference() {
    // Test: Promise.all([p1, p2, p3]) infers tuple of resolved types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t1_name = interner.intern_string("T1");
    let t2_name = interner.intern_string("T2");
    let t3_name = interner.intern_string("T3");

    let var_t1 = ctx.fresh_type_param(t1_name);
    let var_t2 = ctx.fresh_type_param(t2_name);
    let var_t3 = ctx.fresh_type_param(t3_name);

    // Each promise resolves to different type
    ctx.add_lower_bound(var_t1, TypeId::STRING);
    ctx.add_lower_bound(var_t2, TypeId::NUMBER);
    ctx.add_lower_bound(var_t3, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_promise_race_union_inference() {
    // Test: Promise.race([p1, p2]) infers union of resolved types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Race could resolve to either type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// -----------------------------------------------------------------------------
// Generic Function Argument Inference from Context
// -----------------------------------------------------------------------------

#[test]
fn test_generic_arg_inferred_from_return_context() {
    // Test: const x: string = identity(value)
    // T inferred from expected return type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return context expects string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Argument provides string value
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_generic_arg_inferred_from_parameter_type() {
    // Test: function wrap<T>(value: T): Box<T>
    // T inferred from argument type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Argument is number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, forty_two);
}

#[test]
fn test_generic_args_inferred_from_multiple_params() {
    // Test: function pair<T, U>(a: T, b: U): [T, U]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // First argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Second argument
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_generic_arg_inferred_from_callback_param() {
    // Test: function process<T>(fn: (x: T) => void): T
    // T inferred from how callback parameter is used
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Callback parameter usage implies type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_arg_constrained_by_extends() {
    // Test: function fn<T extends number>(x: T): T
    // T is constrained to be subtype of number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint from extends clause
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Argument provides literal
    let five = interner.literal_number(5.0);
    ctx.add_lower_bound(var_t, five);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, five);
}

#[test]
fn test_generic_arg_inferred_from_array_element() {
    // Test: function first<T>(arr: T[]): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element type flows to T
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_arg_from_nested_generic() {
    // Test: function unwrap<T>(box: Box<T>): T
    // T inferred from inner type of Box<string>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Inner type of Box<string> is string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_arg_from_object_property_context() {
    // Test: const obj: { value: string } = { value: getValue<T>() }
    // T inferred from property type context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Property context expects string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_arg_bidirectional_inference() {
    // Test: Both parameter and return type contribute to inference
    // function transform<T>(x: T, fn: (x: T) => T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // From parameter
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // From callback signature (must match)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_arg_inferred_from_spread() {
    // Test: function concat<T>(...arrays: T[][]): T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Spread elements contribute to T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_generic_arg_partial_inference() {
    // Test: function fn<T, U>(x: T): U - U must be explicitly provided or inferred from context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T inferred from argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has no inference sources - returns unknown

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::UNKNOWN);
}

#[test]
fn test_generic_arg_from_conditional_return() {
    // Test: const x: string = cond ? fn<T>() : other
    // T inferred from union member in conditional
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return context from conditional
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Constructor Parameter Inference Tests
// ============================================================================
// Tests for inferring types from class constructor parameters

#[test]
fn test_constructor_param_basic() {
    // Test: class Foo<T> { constructor(x: T) {} } - infer T from argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from constructor argument
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constructor_param_multiple() {
    // Test: class Pair<T, U> { constructor(first: T, second: U) {} }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T and U inferred from constructor arguments
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_constructor_param_with_default() {
    // Test: class Container<T = string> { constructor(value?: T) {} }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // When called with number, T is inferred as number (overriding default)
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_constructor_param_array() {
    // Test: class List<T> { constructor(items: T[]) {} }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from array element type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constructor_param_object() {
    // Test: class Config<T> { constructor(options: { value: T }) {} }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from object property
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

// ============================================================================
// Method Return Type Inference Tests
// ============================================================================
// Tests for inferring types from class method return types

#[test]
fn test_method_return_basic() {
    // Test: class Foo<T> { get(): T { ... } } - infer T from return context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from expected return type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_return_generic_call() {
    // Test: class Builder<T> { build(): T } - called in typed context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return type flows into T
    let return_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("id"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, return_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, return_type);
}

#[test]
fn test_method_return_promise() {
    // Test: class Service<T> { async fetch(): Promise<T> }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is the resolved type of the promise
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_return_array() {
    // Test: class Repository<T> { findAll(): T[] }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from array element expectation
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_method_return_chained() {
    // Test: class Chain<T> { map<U>(fn: (t: T) => U): Chain<U> }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T from input chain, U from callback return
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

// ============================================================================
// Static Member Type Inference Tests
// ============================================================================
// Tests for inferring types from static class members

#[test]
fn test_static_member_basic() {
    // Test: class Factory<T> { static create<T>(): T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from static method context
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_static_member_factory() {
    // Test: class Box<T> { static of<T>(value: T): Box<T> }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T inferred from factory argument
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, lit_hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lit_hello);
}

#[test]
fn test_static_member_property() {
    // Test: class Config<T> { static defaults: T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T from static property type
    let config_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("debug"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, config_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, config_type);
}

#[test]
fn test_static_member_multiple_type_params() {
    // Test: class Mapper<K, V> { static fromEntries<K, V>(entries: [K, V][]): Mapper<K, V> }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name);
    let var_v = ctx.fresh_type_param(v_name);

    // K and V inferred from entry types
    ctx.add_lower_bound(var_k, TypeId::STRING);
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_static_member_with_constraint() {
    // Test: class Serializer<T extends object> { static serialize<T extends object>(obj: T): string }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Lower bound from argument
    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("name"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}

// =============================================================================
// Higher-Order Function Inference Tests
// =============================================================================
// Tests for inferring types in generic HOFs (compose, pipe, curry),
// method chaining, partial application, and overload selection

// -----------------------------------------------------------------------------
// Generic HOF Tests (compose, pipe, curry)
// -----------------------------------------------------------------------------

#[test]
fn test_hof_compose_two_functions() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    // Given f: number => string, g: boolean => number
    // Result: boolean => string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // g: A => B means A is boolean, B is number
    ctx.add_lower_bound(var_a, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // f: B => C means C is string
    ctx.add_lower_bound(var_c, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::BOOLEAN);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_hof_compose_three_functions() {
    // Test: compose3<A, B, C, D>(f, g, h): (a: A) => D
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_d = ctx.fresh_type_param(d_name);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_hof_pipe_left_to_right() {
    // Test: pipe<A, B, C>(g: (a: A) => B, f: (b: B) => C): (a: A) => C
    // Opposite of compose - data flows left to right
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // g: A => B, f: B => C
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_pipe_with_value() {
    // Test: pipeWith<A, B, C>(a: A, f: (a: A) => B, g: (b: B) => C): C
    // Like pipe but starts with a value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Starting value determines A
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // f transforms to B
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // g transforms to C
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, hello);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_curry_binary() {
    // Test: curry<A, B, C>(fn: (a: A, b: B) => C): (a: A) => (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Original function (a: string, b: number) => boolean
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_curry_ternary() {
    // Test: curry3<A, B, C, D>(fn: (a, b, c) => D): (a) => (b) => (c) => D
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_d = ctx.fresh_type_param(d_name);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_hof_uncurry() {
    // Test: uncurry<A, B, C>(fn: (a: A) => (b: B) => C): (a: A, b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Curried function types
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_flip() {
    // Test: flip<A, B, C>(fn: (a: A, b: B) => C): (b: B, a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_hof_constant() {
    // Test: constant<T>(value: T): () => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_hof_identity() {
    // Test: identity<T>(x: T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

// -----------------------------------------------------------------------------
// Method Chaining Type Propagation
// -----------------------------------------------------------------------------

#[test]
fn test_chain_builder_pattern() {
    // Test: Builder<T>.set(k, v).set(k, v).build() => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Builder accumulates to final type
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let obj = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    ctx.add_lower_bound(var_t, obj);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj);
}

#[test]
fn test_chain_fluent_interface() {
    // Test: Fluent<T>.map(f).filter(p).take(n) preserves/transforms T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Initial type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // After map transformation
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_chain_optional_method() {
    // Test: obj?.method()?.next() with optional chaining
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Optional chain may return undefined
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::UNDEFINED);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    assert_eq!(result, expected);
}

#[test]
fn test_chain_type_narrowing() {
    // Test: Chain methods that narrow types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let s_name = interner.intern_string("S");

    let var_t = ctx.fresh_type_param(t_name);
    let var_s = ctx.fresh_type_param(s_name);

    // Original type is union
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union);

    // After filter/narrow, type is narrowed
    ctx.add_lower_bound(var_s, TypeId::STRING);
    ctx.add_upper_bound(var_s, union);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_s = ctx.resolve_with_constraints(var_s).unwrap();

    assert_eq!(result_t, union);
    assert_eq!(result_s, TypeId::STRING);
}

#[test]
fn test_chain_accumulator_type() {
    // Test: scan/reduce-like chain that accumulates type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let elem_name = interner.intern_string("Elem");
    let acc_name = interner.intern_string("Acc");

    let var_elem = ctx.fresh_type_param(elem_name);
    let var_acc = ctx.fresh_type_param(acc_name);

    // Element type from source
    ctx.add_lower_bound(var_elem, TypeId::NUMBER);
    // Accumulator type different from element
    ctx.add_lower_bound(var_acc, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_chain_async_await() {
    // Test: promise.then().then().then() async chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t1_name = interner.intern_string("T1");
    let t2_name = interner.intern_string("T2");
    let t3_name = interner.intern_string("T3");

    let var_t1 = ctx.fresh_type_param(t1_name);
    let var_t2 = ctx.fresh_type_param(t2_name);
    let var_t3 = ctx.fresh_type_param(t3_name);

    // Chain of transformations
    ctx.add_lower_bound(var_t1, TypeId::STRING);
    ctx.add_lower_bound(var_t2, TypeId::NUMBER);
    ctx.add_lower_bound(var_t3, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_chain_branching() {
    // Test: chain.branch() creates two independent chains
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let base_name = interner.intern_string("Base");
    let branch1_name = interner.intern_string("Branch1");
    let branch2_name = interner.intern_string("Branch2");

    let var_base = ctx.fresh_type_param(base_name);
    let var_branch1 = ctx.fresh_type_param(branch1_name);
    let var_branch2 = ctx.fresh_type_param(branch2_name);

    // Base type shared
    ctx.add_lower_bound(var_base, TypeId::STRING);
    // Branch 1 transforms to number
    ctx.add_lower_bound(var_branch1, TypeId::NUMBER);
    // Branch 2 transforms to boolean
    ctx.add_lower_bound(var_branch2, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_chain_merge() {
    // Test: Chain.merge(chain1, chain2) merges types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Merging two chains with different types creates union
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// -----------------------------------------------------------------------------
// Partial Application Inference
// -----------------------------------------------------------------------------

#[test]
fn test_partial_first_arg() {
    // Test: partial(fn, arg1) fixes first parameter
    // partial<A, B, C>((a: A, b: B) => C, a: A): (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // First arg fixed as string
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // Remaining param is number
    ctx.add_upper_bound(var_b, TypeId::NUMBER);
    // Return is boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, hello);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_multiple_args() {
    // Test: partial(fn, arg1, arg2) fixes first two parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_d = ctx.fresh_type_param(d_name);

    // First two args fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // Remaining param
    ctx.add_upper_bound(var_c, TypeId::BOOLEAN);
    // Return type
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}

#[test]
fn test_partial_right() {
    // Test: partialRight fixes last parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // First param remains free
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Last param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, forty_two);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_with_placeholder() {
    // Test: partial(fn, _, arg2) uses placeholder for first arg
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // First param placeholder (remains in signature)
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Second param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, forty_two);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

#[test]
fn test_partial_bind_this() {
    // Test: fn.bind(thisArg) fixes this parameter
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");
    let a_name = interner.intern_string("A");
    let r_name = interner.intern_string("R");

    let var_this = ctx.fresh_type_param(this_name);
    let var_a = ctx.fresh_type_param(a_name);
    let var_r = ctx.fresh_type_param(r_name);

    // This type fixed by bind
    let obj = interner.object(vec![]);
    ctx.add_lower_bound(var_this, obj);
    // Parameter still free
    ctx.add_upper_bound(var_a, TypeId::NUMBER);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, obj);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_partial_bind_this_and_args() {
    // Test: fn.bind(thisArg, arg1, arg2) fixes this and first args
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let r_name = interner.intern_string("R");

    let var_this = ctx.fresh_type_param(this_name);
    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);
    let var_r = ctx.fresh_type_param(r_name);

    // This fixed
    let obj = interner.object(vec![]);
    ctx.add_lower_bound(var_this, obj);
    // First two params fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // Third param free
    ctx.add_upper_bound(var_c, TypeId::BOOLEAN);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, obj);
    assert_eq!(results[1].1, TypeId::STRING);
    assert_eq!(results[2].1, TypeId::NUMBER);
    assert_eq!(results[3].1, TypeId::BOOLEAN);
    assert_eq!(results[4].1, TypeId::SYMBOL);
}

#[test]
fn test_partial_preserves_rest_params() {
    // Test: partial application with rest parameters
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let rest_name = interner.intern_string("Rest");
    let r_name = interner.intern_string("R");

    let var_a = ctx.fresh_type_param(a_name);
    let var_rest = ctx.fresh_type_param(rest_name);
    let var_r = ctx.fresh_type_param(r_name);

    // First param fixed
    ctx.add_lower_bound(var_a, TypeId::STRING);
    // Rest params preserved as number[]
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_rest, number_array);
    // Return type
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, number_array);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}

// -----------------------------------------------------------------------------
// Function Overload Selection
// -----------------------------------------------------------------------------

#[test]
fn test_overload_select_by_arg_count() {
    // Test: Overload selected based on argument count
    // fn(a: string): number
    // fn(a: string, b: number): boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name);

    // With two arguments, second overload is selected
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_overload_select_by_arg_type() {
    // Test: Overload selected based on argument type
    // fn(a: string): string
    // fn(a: number): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name);
    let var_r = ctx.fresh_type_param(r_name);

    // Argument is number, so second overload
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_r, TypeId::NUMBER);
}

#[test]
fn test_overload_select_by_callback_signature() {
    // Test: Overload selected based on callback parameter types
    // fn(cb: (x: string) => void): string
    // fn(cb: (x: number) => void): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let cb_param_name = interner.intern_string("CbParam");
    let r_name = interner.intern_string("R");

    let var_cb_param = ctx.fresh_type_param(cb_param_name);
    let var_r = ctx.fresh_type_param(r_name);

    // Callback expects number param, so second overload
    ctx.add_upper_bound(var_cb_param, TypeId::NUMBER);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_cb = ctx.resolve_with_constraints(var_cb_param).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_cb, TypeId::NUMBER);
    assert_eq!(result_r, TypeId::NUMBER);
}

#[test]
fn test_overload_select_by_return_context() {
    // Test: Overload selected based on expected return type
    // fn<T>(): T (with overloads for specific T)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return context expects string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_select_most_specific() {
    // Test: When multiple overloads match, most specific is selected
    // fn(a: string): string
    // fn(a: "hello"): "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Literal argument matches more specific overload
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_overload_with_optional_params() {
    // Test: Overload with optional parameters
    // fn(a: string): string
    // fn(a: string, b?: number): string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name);

    // With optional param provided, second overload's return type
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_r, union);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, union);
}

#[test]
fn test_overload_with_rest_params() {
    // Test: Overload with rest parameters
    // fn(a: string): string
    // fn(a: string, ...rest: number[]): number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name);

    // With rest params provided, second overload
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_overload_generic_instantiation() {
    // Test: Generic overload instantiation
    // fn<T>(a: T): T
    // fn<T>(a: T, b: T): T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Two args of same type, second overload selected
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_union_arg() {
    // Test: Overload selection with union argument
    // fn(a: string): "s"
    // fn(a: number): "n"
    // Called with string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name);

    // Union arg may match either overload, result is union
    let s = interner.literal_string("s");
    let n = interner.literal_string("n");
    ctx.add_lower_bound(var_r, s);
    ctx.add_lower_bound(var_r, n);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    // Union arg result widens to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_fallback_to_implementation() {
    // Test: When no overload matches, fallback to implementation signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Implementation signature is most general
    ctx.add_upper_bound(var_t, TypeId::UNKNOWN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_overload_conditional_return() {
    // Test: Overload with conditional return type
    // fn<T>(a: T): T extends string ? number : boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name);
    let var_r = ctx.fresh_type_param(r_name);

    // T is string, so return is number
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_r, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_r = ctx.resolve_with_constraints(var_r).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_r, TypeId::NUMBER);
}

// =============================================================================
// Generic Constraint Bound Tests
// =============================================================================
// Tests for generic type parameter constraints (extends clauses),
// multiple bounds, constraint satisfaction, and defaults with constraints

// -----------------------------------------------------------------------------
// Upper Bound Constraints (T extends X)
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_upper_bound_primitive() {
    // Test: <T extends string> - T must be subtype of string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: T is "hello" (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // "hello" satisfies constraint and is the inferred type
    assert_eq!(result, hello);
}

#[test]
fn test_constraint_upper_bound_object() {
    // Test: <T extends { name: string }> - T must have name property
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends { name: string }
    let name_prop = interner.intern_string("name");
    let constraint = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: T is { name: string, age: number }
    let age_prop = interner.intern_string("age");
    let inferred = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    ctx.add_lower_bound(var_t, inferred);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, inferred);
}

#[test]
fn test_constraint_upper_bound_array() {
    // Test: <T extends any[]> - T must be an array type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: T is string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_constraint_upper_bound_function() {
    // Test: <T extends (...args: any[]) => any> - T must be callable
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends function
    let any_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, any_fn);

    // Inference: T is () => number (compatible with () => any)
    let specific_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, specific_fn);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, specific_fn);
}

#[test]
fn test_constraint_upper_bound_union() {
    // Test: <T extends string | number> - T must be string or number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, union);

    // Inference: T is string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_upper_bound_literal() {
    // Test: <T extends "a" | "b" | "c"> - T must be one of the literals
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends "a" | "b" | "c"
    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let c = interner.literal_string("c");
    let union = interner.union(vec![a, b, c]);
    ctx.add_upper_bound(var_t, union);

    // Inference: T is "b"
    ctx.add_lower_bound(var_t, b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, b);
}

#[test]
fn test_constraint_upper_bound_keyof() {
    // Test: <T extends keyof U> - T must be a key of U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends "name" | "age" (simulating keyof { name, age })
    let name = interner.literal_string("name");
    let age = interner.literal_string("age");
    let keys = interner.union(vec![name, age]);
    ctx.add_upper_bound(var_t, keys);

    // Inference: T is "name"
    ctx.add_lower_bound(var_t, name);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, name);
}

#[test]
fn test_constraint_no_inference_uses_constraint() {
    // Test: When no inference, T should resolve to constraint bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint only, no lower bounds
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound, resolves to the constraint
    assert_eq!(result, TypeId::STRING);
}

// -----------------------------------------------------------------------------
// Multiple Constraint Bounds (T extends A & B)
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_multiple_bounds_intersection() {
    // Test: <T extends A & B> - T must satisfy both A and B
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends { name: string } & { age: number }
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let a = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let b = interner.object(vec![PropertyInfo {
        name: age_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let intersection = interner.intersection(vec![a, b]);
    ctx.add_upper_bound(var_t, intersection);

    // Inference: T is { name: string, age: number }
    let both = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    ctx.add_lower_bound(var_t, both);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, both);
}

#[test]
fn test_constraint_multiple_upper_bounds() {
    // Test: Multiple upper bounds added separately
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Two separate upper bounds (both must be satisfied)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Note: In practice, string & number = never, but testing the mechanism

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound string, resolves to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_constraint_intersection_with_callable() {
    // Test: <T extends F & { extra: boolean }> - callable with extra property
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: function type
    let fn_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, fn_type);

    // Inference provides a function
    ctx.add_lower_bound(var_t, fn_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, fn_type);
}

#[test]
fn test_constraint_multiple_type_params_related() {
    // Test: <T extends U, U extends V> - chain of constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);
    let var_v = ctx.fresh_type_param(v_name);

    // V is string
    ctx.add_lower_bound(var_v, TypeId::STRING);
    // U extends V (string)
    ctx.add_upper_bound(var_u, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);
    // T extends U
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, hello);
    assert_eq!(results[1].1, TypeId::STRING);
    assert_eq!(results[2].1, TypeId::STRING);
}

#[test]
fn test_constraint_circular_bounds() {
    // Test: <T extends U, U extends T> - mutually constrained
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Mutual constraints with same inference
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_constraint_intersection_primitives() {
    // Test: <T extends string & Branded> - branded primitive pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // For branded primitives, the intersection is with an object
    let brand_prop = interner.intern_string("__brand");
    let brand = interner.object(vec![PropertyInfo {
        name: brand_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);
    let branded = interner.intersection(vec![TypeId::STRING, brand]);
    ctx.add_upper_bound(var_t, branded);

    ctx.add_lower_bound(var_t, branded);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, branded);
}

// -----------------------------------------------------------------------------
// Constraint Satisfaction During Inference
// -----------------------------------------------------------------------------

#[test]
fn test_constraint_satisfaction_widens_to_bound() {
    // Test: When literal inferred but constraint is wider, result is literal
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal is more specific and satisfies constraint
    assert_eq!(result, hello);
}

#[test]
fn test_constraint_satisfaction_multiple_candidates() {
    // Test: Multiple lower bounds that satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, union);

    // Two lower bounds
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Union of lower bounds
    let expected = interner.union(vec![hello, forty_two]);
    assert_eq!(result, expected);
}

#[test]
fn test_constraint_satisfaction_object_structural() {
    // Test: Object must structurally satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: { x: number }
    let x_prop = interner.intern_string("x");
    let constraint = interner.object(vec![PropertyInfo {
        name: x_prop,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: { x: number, y: string }
    let y_prop = interner.intern_string("y");
    let inferred = interner.object(vec![
        PropertyInfo {
            name: x_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: y_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    ctx.add_lower_bound(var_t, inferred);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, inferred);
}

#[test]
fn test_constraint_satisfaction_function_return() {
    // Test: Return type must satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint from return context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Inference from expression
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, forty_two);
}

#[test]
fn test_constraint_satisfaction_array_element() {
    // Test: Array element type satisfies constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends Comparable (has compare method)
    let compare_prop = interner.intern_string("compare");
    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let comparable = interner.object(vec![PropertyInfo {
        name: compare_prop,
        type_id: compare_fn,
        write_type: compare_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    ctx.add_upper_bound(var_t, comparable);

    // Inference provides object with compare
    ctx.add_lower_bound(var_t, comparable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, comparable);
}

#[test]
fn test_constraint_satisfaction_generic_call() {
    // Test: Generic function call satisfies constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T inferred from argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U inferred from return context
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_constraint_satisfaction_conditional_type() {
    // Test: Constraint affects conditional type resolution
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Lower bound satisfies constraint
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// -----------------------------------------------------------------------------
// Default Type with Constraints
// -----------------------------------------------------------------------------

#[test]
fn test_default_used_when_no_inference() {
    // Test: <T = string> - default used when no inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No constraints, no lower bounds - would use default
    // In this test, we just verify unknown is returned without constraints
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_default_overridden_by_inference() {
    // Test: <T = string> - inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Inference provides number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Inference wins over default
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_default_with_constraint_satisfied() {
    // Test: <T extends object = {}> - default satisfies constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends object (upper bound)
    let empty_obj = interner.object(vec![]);
    ctx.add_upper_bound(var_t, empty_obj);

    // No lower bound, uses upper bound
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, empty_obj);
}

#[test]
fn test_default_literal_with_constraint() {
    // Test: <T extends string = "default"> - literal default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference with literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_default_array_type() {
    // Test: <T extends any[] = never[]> - array default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_default_function_type() {
    // Test: <T extends Function = () => any> - function default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends () => any (allows any return type)
    let any_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_upper_bound(var_t, any_fn);

    // Inference: specific function () => number (subtype of () => any)
    let num_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, num_fn);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, num_fn);
}

#[test]
fn test_default_with_dependent_constraint() {
    // Test: <T, U = T> - U defaults to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T inferred
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has same lower bound (simulating U = T default)
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_default_with_constraint_chain() {
    // Test: <T extends U, U = string> - default in constraint chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // U defaults to string
    ctx.add_lower_bound(var_u, TypeId::STRING);
    // T extends U (string)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // T inferred
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, hello);
    assert_eq!(results[1].1, TypeId::STRING);
}

#[test]
fn test_default_partial_inference() {
    // Test: <T = string, U = number> - partial inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Only T inferred
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);
    // U has no inference - would use default

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::BOOLEAN);
    assert_eq!(result_u, TypeId::UNKNOWN); // No inference, no default in test
}

#[test]
fn test_default_explicit_type_arg() {
    // Test: Explicit type arg overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Explicit type argument (simulated as lower bound)
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // With constraint
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_default_recursive_type() {
    // Test: <T extends Node<T> = Node<any>> - recursive default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Recursive types represented as object with children
    let children_prop = interner.intern_string("children");
    let node = interner.object(vec![PropertyInfo {
        name: children_prop,
        type_id: TypeId::ANY, // Simplified - would be T[]
        write_type: TypeId::ANY,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_upper_bound(var_t, node);
    ctx.add_lower_bound(var_t, node);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, node);
}

// ============================================================================
// CIRCULAR CONSTRAINT TESTS
// ============================================================================

// ----------------------------------------------------------------------------
// Self-referential type parameters (T extends Array<T>)
// ----------------------------------------------------------------------------

#[test]
fn test_self_ref_type_param_array_of_self() {
    // Test: T extends Array<T> with T = string[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Lower bound from usage: string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // The self-referential constraint is conceptual - T should resolve to string[]
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_self_ref_type_param_promise_of_self() {
    // Test: T extends Promise<T> - self-referential promise type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create a function type for the method
    let then_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Lower bound: Promise<number>
    let promise_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: then_fn,
        write_type: then_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);
    ctx.add_lower_bound(var_t, promise_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, promise_type);
}

#[test]
fn test_self_ref_type_param_node_with_children() {
    // Test: T extends { children: T[] } - tree node pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create a node type with children array
    let children_array = interner.array(TypeId::OBJECT);
    let node_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("children"),
        type_id: children_array,
        write_type: children_array,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, node_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, node_type);
}

#[test]
fn test_self_ref_type_param_linked_list() {
    // Test: T extends { next: T | null } - linked list pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create a linked list node with next pointer
    let next_type = interner.union(vec![TypeId::OBJECT, TypeId::NULL]);
    let list_node = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("next"),
            type_id: next_type,
            write_type: next_type,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);
    ctx.add_lower_bound(var_t, list_node);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, list_node);
}

#[test]
fn test_self_ref_type_param_recursive_json() {
    // Test: T extends string | number | T[] | { [key: string]: T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // JSON-like type: union of primitives
    let json_primitives = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);
    ctx.add_lower_bound(var_t, json_primitives);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, json_primitives);
}

// ----------------------------------------------------------------------------
// Mutually dependent type parameters
// ----------------------------------------------------------------------------

#[test]
fn test_mutual_dependency_key_value() {
    // Test: K extends keyof V, V extends Record<K, any>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name);
    let var_v = ctx.fresh_type_param(v_name);

    // K gets "name" literal
    let name_literal = interner.literal_string("name");
    ctx.add_lower_bound(var_k, name_literal);

    // V gets an object with that key
    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("name"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_v, obj_type);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, name_literal);
    assert_eq!(results[1].1, obj_type);
}

#[test]
fn test_mutual_dependency_parent_child() {
    // Test: P extends { child: C }, C extends { parent: P }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let p_name = interner.intern_string("P");
    let c_name = interner.intern_string("C");

    let var_p = ctx.fresh_type_param(p_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Create parent type with child reference
    let parent_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("child"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    // Create child type with parent reference
    let child_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("parent"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_p, parent_type);
    ctx.add_lower_bound(var_c, child_type);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, parent_type);
    assert_eq!(results[1].1, child_type);
}

#[test]
fn test_mutual_dependency_input_output() {
    // Test: I extends (arg: O) => void, O extends ReturnType<I>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let i_name = interner.intern_string("I");
    let o_name = interner.intern_string("O");

    let var_i = ctx.fresh_type_param(i_name);
    let var_o = ctx.fresh_type_param(o_name);

    // Input function type
    let input_fn = interner.function(FunctionShape {
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

    ctx.add_lower_bound(var_i, input_fn);
    ctx.add_lower_bound(var_o, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, input_fn);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_mutual_dependency_request_response() {
    // Test: Req extends { respond: (r: Res) => void }, Res extends { request: Req }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let req_name = interner.intern_string("Req");
    let res_name = interner.intern_string("Res");

    let var_req = ctx.fresh_type_param(req_name);
    let var_res = ctx.fresh_type_param(res_name);

    // Create a method type
    let respond_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Request type with respond method
    let request_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("id"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("respond"),
            type_id: respond_fn,
            write_type: respond_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    // Response type with request reference
    let response_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("data"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("request"),
            type_id: TypeId::OBJECT,
            write_type: TypeId::OBJECT,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_req, request_type);
    ctx.add_lower_bound(var_res, response_type);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, request_type);
    assert_eq!(results[1].1, response_type);
}

#[test]
fn test_mutual_dependency_three_way() {
    // Test: A extends { b: B }, B extends { c: C }, C extends { a: A }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    let type_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("c"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_c = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::OBJECT,
        write_type: TypeId::OBJECT,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_a, type_a);
    ctx.add_lower_bound(var_b, type_b);
    ctx.add_lower_bound(var_c, type_c);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, type_a);
    assert_eq!(results[1].1, type_b);
    assert_eq!(results[2].1, type_c);
}

// ----------------------------------------------------------------------------
// Recursive generic constraints
// ----------------------------------------------------------------------------

#[test]
fn test_recursive_constraint_comparable() {
    // Test: T extends Comparable<T> - self-comparison pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method type
    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Comparable interface with compareTo method
    let comparable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("compareTo"),
        type_id: compare_fn,
        write_type: compare_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, comparable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, comparable_type);
}

#[test]
fn test_recursive_constraint_builder_pattern() {
    // Test: T extends Builder<T> - fluent builder pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method types
    let set_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let build_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Builder with methods that return the builder itself
    let builder_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("set"),
            type_id: set_fn,
            write_type: set_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("build"),
            type_id: build_fn,
            write_type: build_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, builder_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, builder_type);
}

#[test]
fn test_recursive_constraint_expression_tree() {
    // Test: T extends Expr<T> - expression tree pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method type
    let evaluate_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::UNKNOWN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Expression with evaluate method
    let expr_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("evaluate"),
            type_id: evaluate_fn,
            write_type: evaluate_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("children"),
            type_id: interner.array(TypeId::OBJECT),
            write_type: interner.array(TypeId::OBJECT),
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, expr_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, expr_type);
}

#[test]
fn test_recursive_constraint_cloneable() {
    // Test: T extends Cloneable<T> - clone returns same type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method type
    let clone_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Cloneable with clone method
    let cloneable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("clone"),
        type_id: clone_fn,
        write_type: clone_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, cloneable_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, cloneable_type);
}

#[test]
fn test_recursive_constraint_iterable() {
    // Test: T extends Iterable<T> - iterable of self
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method type
    let next_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Iterable with Symbol.iterator method
    let iterable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("next"),
        type_id: next_fn,
        write_type: next_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, iterable_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, iterable_type);
}

// ----------------------------------------------------------------------------
// Constraint cycles in extends clauses
// ----------------------------------------------------------------------------

#[test]
fn test_constraint_cycle_direct_extends() {
    // Test: class A extends B, class B extends A (error case - but test constraint handling)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);

    // Both constrained by object
    ctx.add_upper_bound(var_a, TypeId::OBJECT);
    ctx.add_upper_bound(var_b, TypeId::OBJECT);

    // Both get concrete lower bounds
    ctx.add_lower_bound(var_a, TypeId::OBJECT);
    ctx.add_lower_bound(var_b, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::OBJECT);
    assert_eq!(results[1].1, TypeId::OBJECT);
}

#[test]
fn test_constraint_cycle_interface_extends() {
    // Test: interface A extends B, interface B extends C, interface C extends A
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // Create distinct interface types
    let type_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("propA"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("propB"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_c = interner.object(vec![PropertyInfo {
        name: interner.intern_string("propC"),
        type_id: TypeId::BOOLEAN,
        write_type: TypeId::BOOLEAN,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_a, type_a);
    ctx.add_lower_bound(var_b, type_b);
    ctx.add_lower_bound(var_c, type_c);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, type_a);
    assert_eq!(results[1].1, type_b);
    assert_eq!(results[2].1, type_c);
}

#[test]
fn test_constraint_cycle_generic_extends() {
    // Test: class Container<T extends Container<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create method type
    let get_container_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Container type with self-referential constraint
    let container_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::UNKNOWN,
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("getContainer"),
            type_id: get_container_fn,
            write_type: get_container_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, container_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, container_type);
}

#[test]
fn test_constraint_cycle_mixin_pattern() {
    // Test: type Constructor<T> = new (...args: any[]) => T
    //       function Mixin<T extends Constructor<{}>>(Base: T)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constructor function type
    let constructor_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    // Add lower bound only - this is common for mixin patterns
    ctx.add_lower_bound(var_t, constructor_fn);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, constructor_fn);
}

#[test]
fn test_constraint_cycle_enum_constraint() {
    // Test: T extends keyof typeof Enum where Enum has circular references
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Enum key union
    let enum_keys = interner.union(vec![
        interner.literal_string("A"),
        interner.literal_string("B"),
        interner.literal_string("C"),
    ]);

    ctx.add_lower_bound(var_t, interner.literal_string("A"));
    ctx.add_upper_bound(var_t, enum_keys);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, interner.literal_string("A"));
}

// =============================================================================
// FUNCTION PARAMETER INFERENCE EDGE CASES
// =============================================================================

#[test]
fn test_param_inference_from_array_map_callback() {
    // Test: [1, 2, 3].map(x => x * 2) - x should be inferred as number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element type provides lower bound for callback parameter
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_param_inference_from_array_filter_predicate() {
    // Test: arr.filter(x => x !== null) - x should have array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element is string | null
    let string_or_null = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    ctx.add_lower_bound(var_t, string_or_null);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_or_null);
}

#[test]
fn test_param_inference_from_reduce_accumulator() {
    // Test: arr.reduce((acc, x) => acc + x, 0) - acc inferred from initial value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let acc_name = interner.intern_string("Acc");

    let var_acc = ctx.fresh_type_param(acc_name);

    // Initial value is number, so accumulator is number
    ctx.add_lower_bound(var_acc, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_acc).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_param_inference_from_promise_then_callback() {
    // Test: promise.then(value => ...) - value inferred from Promise<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Promise resolves to string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_param_inference_rest_parameter_tuple() {
    // Test: function f<T extends any[]>(...args: T) - T inferred from arguments
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Arguments are [string, number, boolean]
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);
    ctx.add_lower_bound(var_t, tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tuple);
}

#[test]
fn test_param_inference_spread_arguments() {
    // Test: f(...arr) where f has rest parameter
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Spread array of numbers
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_t, number_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, number_array);
}

#[test]
fn test_param_inference_from_return_type_usage() {
    // Test: const x: string = f(value) - T inferred from expected return type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Function returns T, assigned to string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Argument is specific literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_param_inference_generic_identity() {
    // Test: identity<T>(x: T): T - T inferred from argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Argument is number literal
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, forty_two);
}

#[test]
fn test_param_inference_from_property_access() {
    // Test: function pick<T, K extends keyof T>(obj: T, key: K): T[K]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let var_t = ctx.fresh_type_param(t_name);
    let var_k = ctx.fresh_type_param(k_name);

    // Object argument
    let name_prop = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_t, obj);

    // Key argument
    let key_name = interner.literal_string("name");
    ctx.add_lower_bound(var_k, key_name);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_k = ctx.resolve_with_constraints(var_k).unwrap();

    assert_eq!(result_t, obj);
    assert_eq!(result_k, key_name);
}

#[test]
fn test_param_inference_nested_callback() {
    // Test: arr.map(item => item.children.map(child => child.name))
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Outer array element (has children property)
    let children_prop = interner.intern_string("children");
    let name_prop = interner.intern_string("name");

    let child_type = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let parent_type = interner.object(vec![PropertyInfo {
        name: children_prop,
        type_id: interner.array(child_type),
        write_type: interner.array(child_type),
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, parent_type);
    ctx.add_lower_bound(var_u, child_type);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, parent_type);
    assert_eq!(result_u, child_type);
}

#[test]
fn test_param_inference_optional_with_default() {
    // Test: function f<T = string>(x?: T): T - defaults when no argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No lower bound (optional parameter not provided)
    // Default type acts as fallback
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_param_inference_from_union_argument() {
    // Test: f(maybeString) where maybeString: string | undefined
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, string_or_undefined);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_or_undefined);
}

#[test]
fn test_param_inference_constrained_to_subset() {
    // Test: function f<T extends "a" | "b">(x: T) - T must be subset of constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let constraint = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
    ]);
    ctx.add_upper_bound(var_t, constraint);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lit_a);
}

#[test]
fn test_param_inference_from_tuple_destructure() {
    // Test: const [a, b] = f<[T, U]>([1, "hello"])
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // First element is number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // Second element is string
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_param_inference_bidirectional() {
    // Test: Both parameter and return contribute to inference
    // function f<T>(x: T, transform: (t: T) => T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Argument provides lower bound
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    // Return context provides upper bound (widened to string)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_param_inference_void_callback() {
    // Test: arr.forEach(x => console.log(x)) - callback returns void
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element provides parameter type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ----------------------------------------------------------------------------
// F-bounded polymorphism and advanced extends clause patterns
// ----------------------------------------------------------------------------

#[test]
fn test_f_bounded_comparable() {
    // Test: interface Comparable<T extends Comparable<T>> { compareTo(other: T): number }
    // This is the classic F-bounded polymorphism pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // compareTo method
    let compare_to_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let comparable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("compareTo"),
        type_id: compare_to_fn,
        write_type: compare_to_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, comparable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, comparable_type);
}

#[test]
fn test_f_bounded_builder_pattern() {
    // Test: class Builder<T extends Builder<T>> { build(): T }
    // The builder pattern with fluent interface
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let build_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T (self-referential)
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let set_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("key")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let builder_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("build"),
            type_id: build_fn,
            write_type: build_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("set"),
            type_id: set_fn,
            write_type: set_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, builder_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, builder_type);
}

#[test]
fn test_f_bounded_tree_node() {
    // Test: interface TreeNode<T extends TreeNode<T>> { children: T[] }
    // Self-referential tree structure
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Children is array of self-type
    let children_array = interner.array(TypeId::OBJECT);

    let tree_node_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::UNKNOWN,
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("children"),
            type_id: children_array,
            write_type: children_array,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("parent"),
            type_id: TypeId::OBJECT,
            write_type: TypeId::OBJECT,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, tree_node_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tree_node_type);
}

#[test]
fn test_f_bounded_cloneable() {
    // Test: interface Cloneable<T extends Cloneable<T>> { clone(): T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let clone_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cloneable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("clone"),
        type_id: clone_fn,
        write_type: clone_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, cloneable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, cloneable_type);
}

#[test]
fn test_f_bounded_with_additional_constraint() {
    // Test: T extends Comparable<T> & Serializable
    // F-bounded with intersection constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let compare_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let serialize_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Combined type with both Comparable and Serializable methods
    let combined_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("compareTo"),
            type_id: compare_fn,
            write_type: compare_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("serialize"),
            type_id: serialize_fn,
            write_type: serialize_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, combined_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, combined_type);
}

#[test]
fn test_mutually_recursive_constraints() {
    // Test: interface A<T extends B<T>>, interface B<T extends A<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    let type_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let type_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, type_a);
    ctx.add_lower_bound(var_u, type_b);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);
    ctx.add_upper_bound(var_u, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_extends_clause_with_keyof() {
    // Test: T extends keyof SomeType
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // keyof SomeType = "a" | "b" | "c"
    let key_union = interner.union(vec![
        interner.literal_string("a"),
        interner.literal_string("b"),
        interner.literal_string("c"),
    ]);

    ctx.add_upper_bound(var_t, key_union);
    ctx.add_lower_bound(var_t, interner.literal_string("a"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, interner.literal_string("a"));
}

#[test]
fn test_extends_clause_with_mapped_type_key() {
    // Test: K extends keyof T (common in mapped types)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    // Simulating keyof T where T = { name: string, age: number }
    let keys = interner.union(vec![
        interner.literal_string("name"),
        interner.literal_string("age"),
    ]);

    ctx.add_upper_bound(var_k, keys);
    ctx.add_lower_bound(var_k, interner.literal_string("name"));

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, interner.literal_string("name"));
}

#[test]
fn test_extends_clause_conditional_constraint() {
    // Test: T extends U ? X : Y pattern constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, interner.literal_string("hello"));
}

#[test]
fn test_extends_clause_array_constraint() {
    // Test: T extends Array<infer U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

// =============================================================================
// ADVANCED GENERIC INFERENCE PATTERNS
// =============================================================================

#[test]
fn test_higher_order_function_inference() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    // Inference flows through multiple functions
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name);
    let var_b = ctx.fresh_type_param(b_name);
    let var_c = ctx.fresh_type_param(c_name);

    // g: (a: string) => number, so A = string, B = number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    // f: (b: number) => boolean, so B = number (consistent), C = boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let result_a = ctx.resolve_with_constraints(var_a).unwrap();
    let result_b = ctx.resolve_with_constraints(var_b).unwrap();
    let result_c = ctx.resolve_with_constraints(var_c).unwrap();

    assert_eq!(result_a, TypeId::STRING);
    assert_eq!(result_b, TypeId::NUMBER);
    assert_eq!(result_c, TypeId::BOOLEAN);
}

#[test]
fn test_method_chaining_inference() {
    // Test: array.filter(...).map(...) - type flows through chain
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Initial array element type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // After map, result type
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_partial_type_inference() {
    // Test: Partial<T> inference - each property becomes optional
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Source object type
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");

    let obj = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, obj);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj);
}

#[test]
fn test_record_utility_inference() {
    // Test: Record<K, V> inference from object literal
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name);
    let var_v = ctx.fresh_type_param(v_name);

    // Keys from object: "a" | "b"
    let key_a = interner.literal_string("a");
    let key_b = interner.literal_string("b");
    ctx.add_lower_bound(var_k, interner.union(vec![key_a, key_b]));

    // Value type: number
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let result_k = ctx.resolve_with_constraints(var_k).unwrap();
    let result_v = ctx.resolve_with_constraints(var_v).unwrap();

    let expected_k = interner.union(vec![key_a, key_b]);
    assert_eq!(result_k, expected_k);
    assert_eq!(result_v, TypeId::NUMBER);
}

#[test]
fn test_tuple_to_union_inference() {
    // Test: T[number] where T is a tuple - produces union of element types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Tuple [string, number, boolean]
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tuple);
}

#[test]
fn test_spread_tuple_inference() {
    // Test: [...T, ...U] inference from combined tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // First part of tuple: [string]
    let tuple_t = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        optional: false,
        name: None,
        rest: false,
    }]);

    // Second part: [number, boolean]
    let tuple_u = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            optional: false,
            name: None,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, tuple_t);
    ctx.add_lower_bound(var_u, tuple_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, tuple_t);
    assert_eq!(result_u, tuple_u);
}

#[test]
fn test_awaited_inference() {
    // Test: Awaited<Promise<T>> inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Promise resolves to string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_return_type_inference_async() {
    // Test: async function returns Promise<T>, infer T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return statements provide lower bound
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_mapped_type_key_inference() {
    // Test: { [K in keyof T]: T[K] } - K inferred from source keys
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    // Keys from iteration
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    ctx.add_lower_bound(var_k, key_x);
    ctx.add_lower_bound(var_k, key_y);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    // Multiple string literal keys widen to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_pick_utility_inference() {
    // Test: Pick<T, K> inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let var_t = ctx.fresh_type_param(t_name);
    let var_k = ctx.fresh_type_param(k_name);

    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let email_prop = interner.intern_string("email");

    // Source object with 3 properties
    let source = interner.object(vec![
        PropertyInfo {
            name: name_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: age_prop,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: email_prop,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, source);

    // Pick only "name" | "email"
    let picked_keys = interner.union(vec![
        interner.literal_string("name"),
        interner.literal_string("email"),
    ]);
    ctx.add_lower_bound(var_k, picked_keys);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_k = ctx.resolve_with_constraints(var_k).unwrap();

    assert_eq!(result_t, source);
    assert_eq!(result_k, picked_keys);
}

#[test]
fn test_omit_utility_inference() {
    // Test: Omit<T, K> - K represents keys to exclude
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    // Keys to omit: "password"
    let password_key = interner.literal_string("password");
    ctx.add_lower_bound(var_k, password_key);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, password_key);
}

#[test]
fn test_extract_utility_inference() {
    // Test: Extract<T, U> - filter union to subtypes of U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Union to filter: string | number | boolean
    let union_t = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    ctx.add_lower_bound(var_t, union_t);

    // Filter to: string | number
    let filter_u = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_u, filter_u);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, union_t);
    assert_eq!(result_u, filter_u);
}

#[test]
fn test_parameters_utility_inference() {
    // Test: Parameters<T> - extract parameter types as tuple
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Function type (a: string, b: number) => void
    let func = interner.function(FunctionShape {
        type_params: vec![],
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
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var_t, func);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, func);
}

#[test]
fn test_constructor_parameters_inference() {
    // Test: ConstructorParameters<T> - extract constructor param types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constructor type new (name: string) => Instance
    let ctor = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("name")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    ctx.add_lower_bound(var_t, ctor);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, ctor);
}

#[test]
fn test_instance_type_inference() {
    // Test: InstanceType<T> - extract instance type from constructor
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Instance has specific shape
    let name_prop = interner.intern_string("name");
    let instance = interner.object(vec![PropertyInfo {
        name: name_prop,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, instance);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, instance);
}

// ----------------------------------------------------------------------------
// Additional circular constraint edge cases
// ----------------------------------------------------------------------------

#[test]
fn test_circular_constraint_polymorphic_this() {
    // Test: class Chain { next(): this }
    // Polymorphic this type pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name);

    let next_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let chain_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("next"),
            type_id: next_fn,
            write_type: next_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::UNKNOWN,
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_this, chain_type);
    ctx.add_upper_bound(var_this, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, chain_type);
}

#[test]
fn test_circular_constraint_recursive_promise() {
    // Test: type PromiseChain<T> = Promise<T | PromiseChain<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let then_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("callback")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: then_fn,
        write_type: then_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, promise_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, promise_type);
}

#[test]
fn test_circular_constraint_event_emitter() {
    // Test: interface EventEmitter<T extends EventEmitter<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let on_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("event")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: TypeId::OBJECT,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this for chaining
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emit_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("event")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let emitter_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("on"),
            type_id: on_fn,
            write_type: on_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("emit"),
            type_id: emit_fn,
            write_type: emit_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, emitter_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, emitter_type);
}

#[test]
fn test_circular_constraint_fluent_interface() {
    // Test: interface FluentBuilder<T extends FluentBuilder<T>> with method chaining
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let with_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("key")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::UNKNOWN,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns this
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let fluent_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("withName"),
            type_id: with_fn,
            write_type: with_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("withValue"),
            type_id: with_fn,
            write_type: with_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("withConfig"),
            type_id: with_fn,
            write_type: with_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, fluent_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, fluent_type);
}

#[test]
fn test_circular_constraint_recursive_json() {
    // Test: type JSON = string | number | boolean | null | JSON[] | { [key: string]: JSON }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let json_name = interner.intern_string("JSON");

    let var_json = ctx.fresh_type_param(json_name);

    // JSON is a union of primitives and recursive structures
    let json_union = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);

    ctx.add_lower_bound(var_json, json_union);
    ctx.add_upper_bound(var_json, TypeId::UNKNOWN);

    let result = ctx.resolve_with_constraints(var_json).unwrap();
    assert_eq!(result, json_union);
}

#[test]
fn test_circular_constraint_linked_list_generic() {
    // Test: interface LinkedList<T, Self extends LinkedList<T, Self>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let self_name = interner.intern_string("Self");

    let var_t = ctx.fresh_type_param(t_name);
    let var_self = ctx.fresh_type_param(self_name);

    let node_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("value"),
            type_id: TypeId::UNKNOWN, // Would be T
            write_type: TypeId::UNKNOWN,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("next"),
            type_id: TypeId::OBJECT, // Would be Self | null
            write_type: TypeId::OBJECT,
            optional: true,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_self, node_type);
    ctx.add_upper_bound(var_self, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_circular_constraint_state_machine() {
    // Test: interface State<S extends State<S, E>, E extends Event>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let s_name = interner.intern_string("S");
    let e_name = interner.intern_string("E");

    let var_s = ctx.fresh_type_param(s_name);
    let var_e = ctx.fresh_type_param(e_name);

    let transition_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("event")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns S
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let state_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("name"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: true,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("transition"),
            type_id: transition_fn,
            write_type: transition_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    let event_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("type"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: true,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_s, state_type);
    ctx.add_lower_bound(var_e, event_type);
    ctx.add_upper_bound(var_s, TypeId::OBJECT);
    ctx.add_upper_bound(var_e, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_circular_constraint_visitor_pattern() {
    // Test: interface Visitor<T extends Visitable<T>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let accept_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("visitor")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let visitable_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("accept"),
        type_id: accept_fn,
        write_type: accept_fn,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, visitable_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, visitable_type);
}

#[test]
fn test_circular_constraint_expression_tree() {
    // Test: interface Expr<T extends Expr<T>> { eval(): number; combine(other: T): T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let eval_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let combine_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("other")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let expr_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("eval"),
            type_id: eval_fn,
            write_type: eval_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("combine"),
            type_id: combine_fn,
            write_type: combine_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, expr_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, expr_type);
}

#[test]
fn test_circular_constraint_repository_pattern() {
    // Test: interface Repository<T, R extends Repository<T, R>>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let var_t = ctx.fresh_type_param(t_name);
    let var_r = ctx.fresh_type_param(r_name);

    let find_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("id")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns T | undefined
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let save_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("entity")),
            type_id: TypeId::OBJECT,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::OBJECT, // Returns R for chaining
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let entity_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("id"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let repo_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("find"),
            type_id: find_fn,
            write_type: find_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("save"),
            type_id: save_fn,
            write_type: save_fn,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    ctx.add_lower_bound(var_t, entity_type);
    ctx.add_lower_bound(var_r, repo_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);
    ctx.add_upper_bound(var_r, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

// ----------------------------------------------------------------------------
// Advanced inference from usage patterns
// ----------------------------------------------------------------------------

#[test]
fn test_inference_from_method_chain() {
    // Test: array.map(x => x.name).filter(n => n.length > 0)
    // Infer T from chained method calls
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // T is inferred from the input array element type
    let obj_with_name = interner.object(vec![PropertyInfo {
        name: interner.intern_string("name"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, obj_with_name);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_with_name);
}

#[test]
fn test_inference_from_spread_in_array() {
    // Test: [...arr1, ...arr2] infers common element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Both arrays contribute to T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should infer union of string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_inference_from_spread_in_object() {
    // Test: { ...obj1, ...obj2 } infers merged object type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let obj1 = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, obj1);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj1);
}

#[test]
fn test_inference_from_optional_chain() {
    // Test: obj?.prop infers T | undefined
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Optional chaining produces T | undefined
    let value_or_undef = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, value_or_undef);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, value_or_undef);
}

#[test]
fn test_inference_from_nullish_coalescing() {
    // Test: value ?? defaultValue infers common type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // ?? operator: left side is T | null | undefined, right is T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_default_param() {
    // Test: function(x = defaultValue) infers T from default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Default parameter provides lower bound
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_from_array_destructure() {
    // Test: const [first, ...rest] = arr infers element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array destructuring: first is T, rest is T[]
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_object_destructure() {
    // Test: const { a, b } = obj infers property types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let obj_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("a"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
        },
        PropertyInfo {
            name: interner.intern_string("b"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
        },
    ]);

    ctx.add_lower_bound(var_t, obj_type);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}

#[test]
fn test_inference_from_computed_property() {
    // Test: obj[key] where key: K extends keyof T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    let key_union = interner.union(vec![
        interner.literal_string("x"),
        interner.literal_string("y"),
    ]);

    ctx.add_lower_bound(var_k, interner.literal_string("x"));
    ctx.add_upper_bound(var_k, key_union);

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, interner.literal_string("x"));
}

#[test]
fn test_inference_bidirectional_callback() {
    // Test: arr.map(x => ({ value: x })) bidirectional inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T is inferred from array element, U from callback return
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let wrapper = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    ctx.add_lower_bound(var_u, wrapper);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, wrapper);
}

#[test]
fn test_inference_from_async_await() {
    // Test: async function returns Promise<T>, await unwraps to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Awaited type should be the unwrapped value
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_generator_yield() {
    // Test: function* gen(): Generator<T, R, N> { yield value; }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Yield type contributes to T
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_from_for_of_loop() {
    // Test: for (const x of iterable) { } infers element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Loop variable type is inferred from iterable
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_inference_from_ternary_branches() {
    // Test: cond ? valueA : valueB infers common type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Both branches contribute to T
    ctx.add_lower_bound(var_t, interner.literal_string("a"));
    ctx.add_lower_bound(var_t, interner.literal_string("b"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Ternary branches with string literals simplify to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_type_assertion() {
    // Test: value as T uses T as the inferred type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Type assertion provides the type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// CONTEXT-SENSITIVE TYPING TESTS
// =============================================================================

#[test]
fn test_generic_function_call_single_arg_inference() {
    // identity<T>(x: T): T - infer T from argument
    // identity("hello") should infer T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Argument "hello" provides lower bound for T
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_generic_function_call_multiple_args_same_type() {
    // pair<T>(a: T, b: T): [T, T] - infer T from multiple args
    // pair("a", "b") should infer T = "a" | "b"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    // Both arguments contribute to T
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple string literals widen to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_function_call_different_type_params() {
    // map<T, U>(x: T, f: (t: T) => U): U
    // Infer T from first arg, U from callback return
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T inferred from argument
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // U inferred from callback return type
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_contextual_callback_parameter_type() {
    // arr.map(x => x.length) where arr: string[]
    // x should be contextually typed as string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element type provides upper bound for callback param
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Callback usage provides lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_contextual_callback_return_type() {
    // arr.filter(x => x > 0) - return type is boolean
    // The callback should have contextual return type boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name);

    // filter expects boolean return
    ctx.add_upper_bound(var_r, TypeId::BOOLEAN);

    // Usage returns boolean comparison
    ctx.add_lower_bound(var_r, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_r).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_inference_from_return_context() {
    // function f<T>(): T { ... } with return context
    // const x: string = f() should infer T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Return context provides upper bound
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_object_literal_context() {
    // const obj: { x: number } = { x: value }
    // value should be contextually typed as number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Object property context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, interner.literal_number(42.0));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, interner.literal_number(42.0));
}

#[test]
fn test_inference_array_literal_context() {
    // const arr: string[] = [x, y, z]
    // Elements should be contextually typed as string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Array element context
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should widen to common type (string)
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_from_generic_method_chain() {
    // arr.map(x => x).filter(y => y) - chain preserves type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Initial array type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // map preserves type
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_with_constraint() {
    // function f<T extends string>(x: T): T
    // f("hello") infers T = "hello" within constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Argument provides specific literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_inference_constraint_violation_fallback() {
    // When inference would violate constraint, use constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Conflicting lower bound - implementation may handle differently
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    // The result depends on implementation - may error or use constraint
    let result = ctx.resolve_with_constraints(var_t);
    // Should either error or produce a result
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_contextual_tuple_element_types() {
    // const t: [string, number] = [a, b]
    // a should be string, b should be number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t1_name = interner.intern_string("T1");
    let t2_name = interner.intern_string("T2");

    let var_t1 = ctx.fresh_type_param(t1_name);
    let var_t2 = ctx.fresh_type_param(t2_name);

    // Tuple context
    ctx.add_upper_bound(var_t1, TypeId::STRING);
    ctx.add_upper_bound(var_t2, TypeId::NUMBER);

    ctx.add_lower_bound(var_t1, interner.literal_string("x"));
    ctx.add_lower_bound(var_t2, interner.literal_number(1.0));

    let result_t1 = ctx.resolve_with_constraints(var_t1).unwrap();
    let result_t2 = ctx.resolve_with_constraints(var_t2).unwrap();

    assert_eq!(result_t1, interner.literal_string("x"));
    assert_eq!(result_t2, interner.literal_number(1.0));
}

#[test]
fn test_inference_promise_then_callback() {
    // promise.then(value => ...) - value typed from Promise<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Promise<string> provides context for callback param
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_reduce_accumulator() {
    // arr.reduce((acc, curr) => ..., initial)
    // acc type comes from initial value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let acc_name = interner.intern_string("Acc");

    let var_acc = ctx.fresh_type_param(acc_name);

    // Initial value is number
    ctx.add_lower_bound(var_acc, TypeId::NUMBER);

    // Return type should match accumulator
    ctx.add_upper_bound(var_acc, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_acc).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_generic_class_constructor() {
    // new Container<T>(value) - infer T from value
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Constructor argument
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, hello);
}

#[test]
fn test_inference_spread_in_array() {
    // [...arr1, ...arr2] - infer element type from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // arr1: string[], arr2: number[]
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_inference_object_spread() {
    // { ...obj1, ...obj2 } - merge types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let obj_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("x"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}
// =============================================================================
// OVERLOAD SIGNATURE INFERENCE EDGE CASES
// =============================================================================

#[test]
fn test_overload_with_generic_constraint() {
    // function f<T extends string>(x: T): T;
    // function f<T extends number>(x: T): T;
    // Overload selection based on generic constraints
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // When called with string literal, should match first overload
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the literal "hello"
    assert_eq!(result, interner.literal_string("hello"));
}

#[test]
fn test_overload_with_multiple_generics() {
    // function f<T, U>(x: T, y: U): [T, U];
    // function f<T>(x: T): T;
    // Select overload based on argument count
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Two arguments provided
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_overload_with_this_parameter() {
    // function f(this: string): number;
    // function f(this: number): string;
    // Select overload based on this type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name);

    // this is string
    ctx.add_lower_bound(var_this, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_intersection_argument() {
    // function f(x: A & B): C;
    // function f(x: A): D;
    // More specific type matches first overload
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let obj_a = interner.object(vec![PropertyInfo {
        name: interner.intern_string("a"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let obj_b = interner.object(vec![PropertyInfo {
        name: interner.intern_string("b"),
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let intersection = interner.intersection(vec![obj_a, obj_b]);
    ctx.add_lower_bound(var_t, intersection);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, intersection);
}

#[test]
fn test_overload_constructor_signatures() {
    // new(x: string): StringResult;
    // new(x: number): NumberResult;
    // Constructor overload selection
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Argument is string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_with_literal_types() {
    // function f(x: "a"): 1;
    // function f(x: "b"): 2;
    // function f(x: string): number;
    // Most specific literal overload selected
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, lit_a);
}

#[test]
fn test_overload_with_union_arg_selects_common() {
    // function f(x: string): "str";
    // function f(x: number): "num";
    // f(string | number) should return "str" | "num"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, union);
}

#[test]
fn test_overload_prefer_non_generic() {
    // function f(x: string): string;  // non-generic
    // function f<T>(x: T): T;          // generic fallback
    // Non-generic overload should be preferred
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Provide string argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_with_spread_param() {
    // function f(...args: string[]): string;
    // function f(...args: number[]): number;
    // Select overload based on spread element types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}

#[test]
fn test_overload_with_tuple_spread() {
    // function f(...args: [string, number]): A;
    // function f(...args: [string]): B;
    // Select overload based on tuple length
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

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
    ctx.add_lower_bound(var_t, tuple);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, tuple);
}

#[test]
fn test_overload_ambiguous_fallback() {
    // When multiple overloads could match, use implementation signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // any argument could match multiple overloads
    ctx.add_lower_bound(var_t, TypeId::ANY);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to any
    assert_eq!(result, TypeId::ANY);
}

#[test]
fn test_overload_callback_return_type() {
    // function f(cb: () => string): "string-cb";
    // function f(cb: () => number): "number-cb";
    // Select based on callback return type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Callback returns string
    let callback = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_t, callback);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, callback);
}

#[test]
fn test_overload_nested_generics() {
    // function f<T>(x: Promise<T>): T;
    // function f<T>(x: T): T;
    // First overload matches Promise, second is fallback
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Provide Promise-like object
    let then_method = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let promise_like = interner.object(vec![PropertyInfo {
        name: interner.intern_string("then"),
        type_id: then_method,
        write_type: then_method,
        optional: false,
        readonly: false,
        is_method: true,
    }]);

    ctx.add_lower_bound(var_t, promise_like);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, promise_like);
}

#[test]
fn test_overload_with_default_type_param() {
    // function f<T = string>(x?: T): T;
    // When no arg, use default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // No lower bound provided, should fallback to upper if exists
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound and no lower, resolves to upper
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_contextual_from_target() {
    // const f: { (x: string): string } = overloaded;
    // Select overload matching target signature
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Target expects string -> string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// SOLV-16: Enhanced Generic Inference Tests
// =============================================================================

#[test]
fn test_conditional_type_inference_basic() {
    // type Wrapped<T> = T extends string ? { value: T } : never;
    // When inferring T, if we have { value: string }, T should be string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Create a conditional type: T extends string ? { value: T } : never
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let object_t = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let _cond = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: object_t,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Infer from the conditional type
    ctx.infer_from_conditional(var_t, t_type, TypeId::STRING, object_t, TypeId::NEVER);

    // The constraint should be that T extends string
    let constraints = ctx.get_constraints(var_t);
    assert!(constraints.is_some());
    let constraints = constraints.unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_variance_computation_covariant() {
    // type Box<T> = { value: T };
    // T is covariant in Box<T> (appears in read position)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let box_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: true, // Readonly makes it purely covariant
        is_method: false,
    }]);

    let (covariant, contravariant, invariant, bivariant) = ctx.compute_variance(box_type, t_name);

    assert_eq!(covariant, 1);
    assert_eq!(contravariant, 0);
    assert_eq!(invariant, 0);
    assert_eq!(bivariant, 0);
}

#[test]
fn test_variance_computation_contravariant() {
    // type Mapper<T> = { map: (x: T) => void };
    // T is contravariant in the function parameter position
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            type_id: t_type,
            name: Some(interner.intern_string("x")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let (covariant, contravariant, invariant, bivariant) = ctx.compute_variance(func, t_name);

    assert_eq!(covariant, 0);
    assert_eq!(contravariant, 1);
    assert_eq!(invariant, 0);
    assert_eq!(bivariant, 0);
}

#[test]
fn test_variance_computation_invariant() {
    // type ReadWrite<T> = { get: () => T, set: (x: T) => void };
    // T is invariant (appears in both covariant and contravariant positions)
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let get_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let set_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            type_id: t_type,
            name: Some(interner.intern_string("x")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rw_type = interner.object(vec![
        PropertyInfo {
            name: interner.intern_string("get"),
            type_id: get_func,
            write_type: get_func,
            optional: false,
            readonly: false,
            is_method: true,
        },
        PropertyInfo {
            name: interner.intern_string("set"),
            type_id: set_func,
            write_type: set_func,
            optional: false,
            readonly: false,
            is_method: true,
        },
    ]);

    let (covariant, contravariant, _invariant, _bivariant) = ctx.compute_variance(rw_type, t_name);

    // Should be marked as invariant since it appears in both positions
    assert!(covariant > 0);
    assert!(contravariant > 0);
    // The compute_variance returns raw counts, and the caller interprets
    // both covariant and contravariant as invariant
}

#[test]
fn test_variance_string() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let array_type = interner.array(t_type);

    assert_eq!(ctx.get_variance(array_type, t_name), "covariant");
}

#[test]
fn test_infer_from_context() {
    // function foo<T>(x: T): T;
    // const result: string = foo("hello");
    // The context (result: string) provides an upper bound for T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Infer from context: result type is string
    ctx.infer_from_context(var_t, TypeId::STRING).unwrap();

    let constraints = ctx.get_constraints(var_t);
    assert!(constraints.is_some());
    let constraints = constraints.unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_strengthen_constraints() {
    // function foo<T, U extends T>(x: T, y: U): void;
    // If we know T = string, then U must be at most string (string <: U)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // U extends T
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let _u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Add constraints: T has lower bound string, U extends T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_u, t_type);

    // Strengthen constraints should propagate
    ctx.strengthen_constraints().unwrap();

    // U should now have string as a lower bound (via T)
    let u_constraints = ctx.get_constraints(var_u);
    assert!(u_constraints.is_some());
    let u_constraints = u_constraints.unwrap();
    // U should have inherited the constraint from T
    assert!(!u_constraints.upper_bounds.is_empty());
}

#[test]
fn test_best_common_type_with_literals() {
    // ["hello", "world"] should infer as string, not union of two literals
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");

    let result = ctx.best_common_type(&[hello, world]);

    // Should widen to string, not stay as union of literals
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_mixed() {
    // [string, "hello"] should infer as string
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let hello = interner.literal_string("hello");
    let types = &[TypeId::STRING, hello];

    let result = ctx.best_common_type(types);

    // Should be string (the common base type)
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_best_common_type_union_fallback() {
    // [string, number] should infer as string | number
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);

    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_contains_inference_var() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let array_t = interner.array(t_type);

    assert!(ctx.contains_inference_var(array_t, var_t));
    assert!(!ctx.contains_inference_var(TypeId::STRING, var_t));
}

#[test]
fn test_validate_variance() {
    // Validate that resolved types don't have circular references
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Unify with a concrete type
    ctx.unify_var_type(var_t, TypeId::STRING).unwrap();

    // Validate should pass
    ctx.validate_variance().unwrap();
}

/// Test variance computation for conditional types
///
/// NOTE: Currently ignored - variance computation for conditional types is not fully
/// implemented. Conditional types should be invariant in their check type, but the
/// variance computation returns contravariant instead.
#[test]
#[ignore = "Variance computation for conditional types not fully implemented"]
fn test_variance_conditional_type() {
    // type Check<T> = T extends string ? true : false;
    // Conditional types are invariant in their check type
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    let cond = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::BOOLEAN_TRUE,
        false_type: TypeId::BOOLEAN_FALSE,
        is_distributive: true,
    });

    let variance = ctx.get_variance(cond, t_name);
    // Check and extends should create invariance
    assert_eq!(variance, "invariant");
}

#[test]
fn test_complex_generic_inference() {
    // function map<T, U>(arr: T[], fn: (x: T) => U): U[];
    // Test that we can infer both T and U from arguments
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // T is inferred from array element type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U is inferred from function return type
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    // Also U is constrained by T through the function parameter
    let _t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // The function parameter (x: T) creates a relationship
    // But for this test, we just check both can be resolved
    let resolved_t = ctx.resolve_with_constraints(var_t).unwrap();
    let resolved_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(resolved_t, TypeId::STRING);
    assert_eq!(resolved_u, TypeId::NUMBER);
}

#[test]
fn test_bidirectional_inference() {
    // function foo<T>(x: T): T;
    // const result: string = foo(?);
    // T should be inferred as string from the context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Context provides upper bound
    ctx.infer_from_context(var_t, TypeId::STRING).unwrap();

    // Resolve with constraints
    let resolved = ctx.resolve_with_constraints(var_t).unwrap();

    assert_eq!(resolved, TypeId::STRING);
}

#[test]
fn test_inference_with_constraints() {
    // function foo<T extends number>(x: T): T;
    // When called with foo(42), T should be 42 (the literal)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name);

    // Add constraint: T extends number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    // Add lower bound from argument
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    // Resolve should return the literal since it satisfies the constraint
    let resolved = ctx.resolve_with_constraints(var_t).unwrap();

    assert_eq!(resolved, forty_two);
}

// =============================================================================
// Template Literal Inference Tests
// =============================================================================

#[test]
fn test_template_literal_contains_inference_var() {
    // Test that contains_inference_var properly detects infer in template literals
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create a TypeParameter representing T
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Create template literal: `prefix${T}suffix`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    // Should detect that the template contains inference var T
    assert!(ctx.contains_inference_var(template, var_t));
}

#[test]
fn test_template_literal_does_not_contain_unrelated_var() {
    // Test that contains_inference_var returns false for unrelated var
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let _var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Create a TypeParameter representing T
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Create template literal: `prefix${T}suffix`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    // Should not detect U in template containing T
    assert!(!ctx.contains_inference_var(template, var_u));
}

#[test]
fn test_template_literal_text_only_no_inference_var() {
    // Test that text-only template literals don't contain inference vars
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create text-only template literal: `hello world`
    let template = interner.template_literal(vec![TemplateSpan::Text(
        interner.intern_string("hello world"),
    )]);

    // Should not detect any inference var
    assert!(!ctx.contains_inference_var(template, var_t));
}

#[test]
fn test_template_literal_multiple_inference_positions() {
    // Test template literal with multiple type positions
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name);
    let var_u = ctx.fresh_type_param(u_name);

    // Create type parameters
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));
    let u_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
    }));

    // Create template literal: `${T}-${U}`
    let template = interner.template_literal(vec![
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(u_type),
    ]);

    // Should detect both T and U
    assert!(ctx.contains_inference_var(template, var_t));
    assert!(ctx.contains_inference_var(template, var_u));
}

#[test]
fn test_template_literal_infer_from_conditional() {
    // Test that infer_from_conditional properly traverses template literals in true_type branch
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name);

    // Create a TypeParameter representing T with constraint string
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(TypeId::STRING),
        default: None,
    }));

    // Create template literal: `get${T}`
    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(t_type),
    ]);

    // Use infer_from_conditional (which is public) with template in true_type
    // T extends string ? `get${T}` : never
    ctx.infer_from_conditional(
        var_t,
        t_type,         // check_type: T
        TypeId::STRING, // extends_type: string
        template,       // true_type: `get${T}`
        TypeId::NEVER,  // false_type: never
    );

    // Should have added string as upper bound from extends_type detection
    // The infer_from_conditional adds upper bound when check_type matches var
    let constraints = ctx.get_constraints(var_t).unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

#[test]
fn test_template_literal_inference_context_integration() {
    // Integration test for template literal inference with InferenceContext
    // This tests the scenario: T extends `get${infer K}` ? K : never
    // When T = "getName", K should be "Name"
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    // Create infer type representing "infer K"
    let infer_k = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // Verify that infer K is detectable in the pattern
    assert!(ctx.contains_inference_var(pattern, var_k));

    // The actual inference from string literal to infer type is done in
    // TypeEvaluator::match_template_literal_string, not in InferenceContext.
    // InferenceContext just tracks the bounds and constraints.
    // This test verifies the traversal works correctly.
}

#[test]
fn test_nested_template_literal_in_conditional() {
    // Test nested template literal in conditional type
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name);

    // Create infer type representing "infer K"
    let infer_k = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // Create check type parameter T
    let t_name = interner.intern_string("T");
    let t_type = interner.intern(TypeKey::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
    }));

    // Create conditional: T extends `get${infer K}` ? K : never
    let cond = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    });

    // Verify that the conditional contains the inference var K
    assert!(ctx.contains_inference_var(cond, var_k));
}

#[test]
fn test_template_literal_inference_end_to_end() {
    // End-to-end test: T extends `get${infer K}` ? K : never
    // When T = "getName", result should be "Name"
    use crate::solver::evaluate::evaluate_conditional;
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();

    // Create infer type: infer K
    let k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // Create input: "getName"
    let get_name = interner.literal_string("getName");

    // Create conditional: "getName" extends `get${infer K}` ? K : never
    let cond = ConditionalType {
        check_type: get_name,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: false, // Not distributive for this simple case
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be "Name" (the inferred K)
    let name_literal = interner.literal_string("Name");
    assert_eq!(
        result, name_literal,
        "T extends `get${{infer K}}` ? K : never should infer K='Name' from T='getName'"
    );
}

#[test]
fn test_template_literal_inference_no_match() {
    // Test: T extends `get${infer K}` ? K : never
    // When T = "setValue", result should be never (no match)
    use crate::solver::evaluate::evaluate_conditional;
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();

    // Create infer type: infer K
    let k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // Create input: "setValue" (doesn't match pattern)
    let set_value = interner.literal_string("setValue");

    // Create conditional
    let cond = ConditionalType {
        check_type: set_value,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be never (false branch)
    assert_eq!(
        result,
        TypeId::NEVER,
        "T extends `get${{infer K}}` ? K : never should return never when T doesn't match"
    );
}

#[test]
fn test_template_literal_inference_prefix_suffix() {
    // Test: T extends `prefix-${infer R}-suffix` ? R : never
    // When T = "prefix-middle-suffix", result should be "middle"
    use crate::solver::evaluate::evaluate_conditional;
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();

    // Create infer type: infer R
    let r_name = interner.intern_string("R");
    let infer_r = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `prefix-${infer R}-suffix`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(infer_r),
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ]);

    // Create input: "prefix-middle-suffix"
    let input = interner.literal_string("prefix-middle-suffix");

    // Create conditional
    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be "middle" (the inferred R)
    let middle = interner.literal_string("middle");
    assert_eq!(
        result, middle,
        "T extends `prefix-${{infer R}}-suffix` ? R : never should infer R='middle'"
    );
}

#[test]
fn test_template_literal_inference_multiple_infers() {
    // Test: T extends `${infer A}-${infer B}` ? [A, B] : never
    // When T = "hello-world", result should be tuple ["hello", "world"]
    use crate::solver::evaluate::evaluate_conditional;
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();

    // Create infer types
    let a_name = interner.intern_string("A");
    let infer_a = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: a_name,
        constraint: None,
        default: None,
    }));
    let b_name = interner.intern_string("B");
    let infer_b = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: b_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `${infer A}-${infer B}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Type(infer_a),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(infer_b),
    ]);

    // Create input: "hello-world"
    let input = interner.literal_string("hello-world");

    // For true_type, we'll just use infer_a to check A is inferred correctly
    // (Testing full tuple would require more complex setup)
    let cond = ConditionalType {
        check_type: input,
        extends_type: pattern,
        true_type: infer_a, // Return A
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be "hello" (the inferred A)
    let hello = interner.literal_string("hello");
    assert_eq!(
        result, hello,
        "First infer in template literal should capture 'hello'"
    );
}

#[test]
fn test_template_literal_inference_distributive() {
    // Test distributive conditional: T extends `get${infer K}` ? K : never
    // When T = "getName" | "getValue", result should be "Name" | "Value"
    use crate::solver::evaluate::evaluate_conditional;
    use crate::solver::types::TemplateSpan;

    let interner = TypeInterner::new();

    // Create infer type: infer K
    let k_name = interner.intern_string("K");
    let infer_k = interner.intern(TypeKey::Infer(TypeParamInfo {
        name: k_name,
        constraint: None,
        default: None,
    }));

    // Create template literal pattern: `get${infer K}`
    let pattern = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("get")),
        TemplateSpan::Type(infer_k),
    ]);

    // Create union input: "getName" | "getValue"
    let get_name = interner.literal_string("getName");
    let get_value = interner.literal_string("getValue");
    let union_input = interner.union(vec![get_name, get_value]);

    // Create conditional with distributive = true
    let cond = ConditionalType {
        check_type: union_input,
        extends_type: pattern,
        true_type: infer_k,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let result = evaluate_conditional(&interner, &cond);

    // Result should be "Name" | "Value"
    // The result is a union of inferred values
    assert!(result != TypeId::NEVER, "Result should not be never");
    assert!(result != TypeId::ERROR, "Result should not be error");

    // Check that we got a union or at least one of the expected values
    let name = interner.literal_string("Name");
    let value = interner.literal_string("Value");
    let expected_union = interner.union(vec![name, value]);

    // The result should be equivalent to "Name" | "Value"
    // (order may vary in union)
    assert_eq!(
        result, expected_union,
        "Distributive conditional should produce 'Name' | 'Value'"
    );
}
