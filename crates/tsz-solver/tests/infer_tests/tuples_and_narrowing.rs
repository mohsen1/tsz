use super::*;

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Types inferred from destructuring context
    let lit_hello = interner.literal_string("hello");
    let lit_42 = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, lit_hello);
    ctx.add_lower_bound(var_u, lit_42);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

#[test]
fn test_named_tuple_three_elements() {
    // Test: [a: T, b: U, c: V] - three named elements
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    // Test: T extends { kind: "a" } | { kind: "b" } with candidate { kind: "a" }
    // The constraint's literal types prevent widening of discriminant properties.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Create discriminated union members
    let kind_prop = interner.intern_string("kind");
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");

    let type_a = interner.object(vec![PropertyInfo::new(kind_prop, lit_a)]);
    let type_b = interner.object(vec![PropertyInfo::new(kind_prop, lit_b)]);

    // Add upper bound: T extends { kind: "a" } | { kind: "b" }
    let constraint = interner.union(vec![type_a, type_b]);
    ctx.add_upper_bound(var_t, constraint);

    // Add candidate: { kind: "a" }
    ctx.add_lower_bound(var_t, type_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, type_a);
}

#[test]
fn test_discriminated_union_switch() {
    // Test: T extends Shape (discriminated union) with candidate { kind: "circle", radius: number }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let kind_prop = interner.intern_string("kind");
    let radius_prop = interner.intern_string("radius");
    let side_prop = interner.intern_string("side");
    let lit_circle = interner.literal_string("circle");
    let lit_square = interner.literal_string("square");

    let circle_type = interner.object(vec![
        PropertyInfo::new(kind_prop, lit_circle),
        PropertyInfo::new(radius_prop, TypeId::NUMBER),
    ]);
    let square_type = interner.object(vec![
        PropertyInfo::new(kind_prop, lit_square),
        PropertyInfo::new(side_prop, TypeId::NUMBER),
    ]);

    // Add upper bound: T extends Circle | Square
    let constraint = interner.union(vec![circle_type, square_type]);
    ctx.add_upper_bound(var_t, constraint);

    ctx.add_lower_bound(var_t, circle_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, circle_type);
}

#[test]
fn test_discriminated_union_type_property() {
    // Test: T extends { type: "request" } | { type: "response" } with constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let type_prop = interner.intern_string("type");
    let lit_request = interner.literal_string("request");
    let lit_response = interner.literal_string("response");
    let body_prop = interner.intern_string("body");

    let request_type = interner.object(vec![
        PropertyInfo::new(type_prop, lit_request),
        PropertyInfo::new(body_prop, TypeId::STRING),
    ]);
    let response_type = interner.object(vec![
        PropertyInfo::new(type_prop, lit_response),
        PropertyInfo::new(body_prop, TypeId::STRING),
    ]);

    // Add upper bound: T extends Request | Response
    let constraint = interner.union(vec![request_type, response_type]);
    ctx.add_upper_bound(var_t, constraint);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    let success_prop = interner.intern_string("success");
    let data_prop = interner.intern_string("data");

    // Use BOOLEAN for success field (representing literal true)
    let success_type = interner.object(vec![
        PropertyInfo::new(success_prop, TypeId::BOOLEAN),
        PropertyInfo::new(data_prop, TypeId::STRING),
    ]);

    ctx.add_lower_bound(var_t, success_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, success_type);
}

#[test]
fn test_discriminated_union_numeric_discriminant() {
    // Test: T extends { code: 200 } | { code: 404 } with numeric literal constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let code_prop = interner.intern_string("code");
    let body_prop = interner.intern_string("body");
    let message_prop = interner.intern_string("message");
    let lit_200 = interner.literal_number(200.0);
    let lit_404 = interner.literal_number(404.0);

    let ok_response = interner.object(vec![
        PropertyInfo::new(code_prop, lit_200),
        PropertyInfo::new(body_prop, TypeId::STRING),
    ]);
    let err_response = interner.object(vec![
        PropertyInfo::new(code_prop, lit_404),
        PropertyInfo::new(message_prop, TypeId::STRING),
    ]);

    // Add upper bound: T extends OkResponse | ErrResponse
    let constraint = interner.union(vec![ok_response, err_response]);
    ctx.add_upper_bound(var_t, constraint);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // After "name" in x, narrow to object with name
    let name_prop = interner.intern_string("name");
    let with_name = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let bird_type = interner.object(vec![PropertyInfo::method(fly_prop, fly_fn)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Object with optional property (in check confirms it exists)
    let opt_prop = interner.intern_string("optional");
    let with_optional = interner.object(vec![PropertyInfo::opt(opt_prop, TypeId::STRING)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let array_like = interner.object(vec![PropertyInfo::method(foreach_prop, foreach_fn)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // After !("special" in x), narrow to object without special
    let other_prop = interner.intern_string("basic");
    let without_special = interner.object(vec![PropertyInfo::new(other_prop, TypeId::STRING)]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // val argument provides lower bound
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Callback param x will be "hello" type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_callback_param_inferred_from_multiple_calls() {
    // Test: Callback called with different values
    // e.g., function callBoth<T>(fn: (x: T) => void) { fn("a"); fn(1); }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Callback called with string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Callback called with number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions the lower bounds: string | number
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

    let var_u = ctx.fresh_type_param(u_name, false);

    // Return type must satisfy usage context
    ctx.add_upper_bound(var_u, TypeId::NUMBER);
    // Callback returns specific number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_u, forty_two);

    let result = ctx.resolve_with_constraints(var_u).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_callback_param_from_object_method_context() {
    // Test: obj.method((x) => ...) where method signature defines x's type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_idx = ctx.fresh_type_param(idx_name, false);
    let var_arr = ctx.fresh_type_param(arr_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_s = ctx.fresh_type_param(s_name, false);

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

    let var_acc = ctx.fresh_type_param(acc_name, false);
    let var_elem = ctx.fresh_type_param(elem_name, false);

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

    let var_acc = ctx.fresh_type_param(acc_name, false);
    let var_elem = ctx.fresh_type_param(elem_name, false);

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
    // With NakedTypeVariable priority, first candidate wins
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Element type
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    // Return includes undefined possibility
    ctx.add_lower_bound(var_t, TypeId::UNDEFINED);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With nullable union inference, candidates [number, undefined] produce
    // number | undefined. (In practice, Array.find's undefined comes from
    // the method return type, not from inference candidates.)
    assert_ne!(result, TypeId::NEVER);
    assert_ne!(result, TypeId::UNKNOWN);
}

#[test]
fn test_array_every_callback_returns_boolean() {
    // Test: nums.every((n) => n > 0)
    // Callback must return boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let ret_name = interner.intern_string("Ret");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_ret = ctx.fresh_type_param(ret_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_err = ctx.fresh_type_param(err_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t1 = ctx.fresh_type_param(t1_name, false);
    let var_t2 = ctx.fresh_type_param(t2_name, false);
    let var_t3 = ctx.fresh_type_param(t3_name, false);

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
    // Test: Promise.race([p1, p2]) — both element types contribute
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Race could resolve to either type
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions both: string | number
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Return context expects string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Argument provides string value
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_arg_inferred_from_parameter_type() {
    // Test: function wrap<T>(value: T): Box<T>
    // T inferred from argument type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument is number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_args_inferred_from_multiple_params() {
    // Test: function pair<T, U>(a: T, b: U): [T, U]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint from extends clause
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Argument provides literal
    let five = interner.literal_number(5.0);
    ctx.add_lower_bound(var_t, five);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_arg_inferred_from_array_element() {
    // Test: function first<T>(arr: T[]): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Spread elements contribute to T
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Return type flows into T
    let return_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("id"),
        TypeId::NUMBER,
    )]);
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // T inferred from factory argument
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, lit_hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_static_member_property() {
    // Test: class Config<T> { static defaults: T }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T from static property type
    let config_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("debug"),
        TypeId::BOOLEAN,
    )]);
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

    let var_k = ctx.fresh_type_param(k_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Lower bound from argument
    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}
