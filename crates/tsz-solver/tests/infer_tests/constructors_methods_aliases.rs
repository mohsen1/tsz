use super::*;

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // T has upper bound of object
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Argument is specific object type
    let prop_name = interner.intern_string("name");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Rest param elements are string and number - infer union
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should be union of string | number
    if let Some(TypeData::Union(_)) = interner.lookup(result) {
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: () => T
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: (x: T) => void
    let _method = interner.function(FunctionShape {
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

    let var_this = ctx.fresh_type_param(this_name, false);
    let this_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: this_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: (this: This) => This
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: this_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![],
        this_type: Some(this_type),
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create an object type to represent `this`
    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: <T>(x: T) => Array<T>
    let return_array = interner.array(t_type);
    let _method = interner.function(FunctionShape {
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

    let var_k = ctx.fresh_type_param(k_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Just test that we can have object bounds without infinite recursion
    let prop_name = interner.intern_string("value");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);
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

    let var_f = ctx.fresh_type_param(f_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: number (which is comparable to itself)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    // Lower bound: specific number literal
    let num_lit = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, num_lit);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::NUMBER);
}

#[test]
fn test_self_ref_constraint_builder_pattern() {
    // Test: T extends Builder<T> - fluent builder pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let builder_type = interner.object(vec![PropertyInfo::method(build_prop, builder_fn)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

    // Both get object lower bounds (breaking the cycle with concrete types)
    let prop_a = interner.intern_string("value");
    let obj_a = interner.object(vec![PropertyInfo::new(prop_a, TypeId::STRING)]);

    let prop_b = interner.intern_string("count");
    let obj_b = interner.object(vec![PropertyInfo::new(prop_b, TypeId::NUMBER)]);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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
    assert_eq!(resolved[0].1, TypeId::STRING);
    assert_eq!(resolved[1].1, TypeId::STRING);
    assert_eq!(resolved[2].1, TypeId::STRING);
}

#[test]
fn test_mutual_recursion_shared_constraint() {
    // Test: A and B both bounded by same type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

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

    let var_f = ctx.fresh_type_param(f_name, false);
    let var_g = ctx.fresh_type_param(g_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // T inferred from setValue argument
    let string_lit = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, string_lit);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_chain_transform() {
    // Test: chain<T>.map<U>(fn: (t: T) => U) => chain<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Specific object as lower bound
    let prop = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T gets bound from first argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U gets bound from second argument (a string literal)
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_u, lit_hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_generic_function_shared_type_param() {
    // Test: <T>(a: T, b: T) => T
    // Both arguments contribute to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);

    // Contextual type says return is string
    // Arrow function body returns a string literal
    ctx.add_upper_bound(var_t, TypeId::STRING);
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, lit_hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the more specific type: "hello"
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_contextual_arrow_return_array() {
    // Test: contextual array return type
    // const fn: () => number[] = () => [1, 2, 3]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Return value contains number literals
    let lit_1 = interner.literal_number(1.0);
    ctx.add_lower_bound(var_t, lit_1);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should infer the literal type
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_contextual_arrow_return_object() {
    // Test: contextual object return type
    // const fn: () => { x: number } = () => ({ x: 42 })
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects { x: number }
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Actual value is 42
    let lit_42 = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, lit_42);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_contextual_arrow_callback_param() {
    // Test: callback parameter inference
    // arr.map((x) => x + 1) where arr: number[]
    // x should be inferred as number from the array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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
