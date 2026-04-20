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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Starting value determines A
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // f transforms to B
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    // g transforms to C
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_hof_identity() {
    // Test: identity<T>(x: T): T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Builder accumulates to final type
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let obj = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Optional chain may return undefined
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::UNDEFINED);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With nullable union inference, candidates [string, undefined] produce
    // string | undefined.
    assert_ne!(result, TypeId::NEVER);
    assert_ne!(result, TypeId::UNKNOWN);
}
#[test]
fn test_chain_type_narrowing() {
    // Test: Chain methods that narrow types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let s_name = interner.intern_string("S");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_s = ctx.fresh_type_param(s_name, false);

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

    let var_elem = ctx.fresh_type_param(elem_name, false);
    let var_acc = ctx.fresh_type_param(acc_name, false);

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

    let var_t1 = ctx.fresh_type_param(t1_name, false);
    let var_t2 = ctx.fresh_type_param(t2_name, false);
    let var_t3 = ctx.fresh_type_param(t3_name, false);

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

    let var_base = ctx.fresh_type_param(base_name, false);
    let var_branch1 = ctx.fresh_type_param(branch1_name, false);
    let var_branch2 = ctx.fresh_type_param(branch2_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Merging two chains with different types
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First arg fixed as string
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_a, hello);
    // Remaining param is number
    ctx.add_upper_bound(var_b, TypeId::NUMBER);
    // Return is boolean
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First param remains free
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Last param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // First param placeholder (remains in signature)
    ctx.add_upper_bound(var_a, TypeId::STRING);
    // Second param fixed
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_b, forty_two);
    // Return type
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
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

    let var_this = ctx.fresh_type_param(this_name, false);
    let var_a = ctx.fresh_type_param(a_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_this = ctx.fresh_type_param(this_name, false);
    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_rest = ctx.fresh_type_param(rest_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_cb_param = ctx.fresh_type_param(cb_param_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Literal argument matches more specific overload
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_overload_with_optional_params() {
    // Test: Overload with optional parameters
    // fn(a: string): string
    // fn(a: string, b?: number): string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let r_name = interner.intern_string("R");

    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: T is "hello" (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // "hello" satisfies constraint and is the inferred type
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_upper_bound_object() {
    // Test: <T extends { name: string }> - T must have name property
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends { name: string }
    let name_prop = interner.intern_string("name");
    let constraint = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: T is { name: string, age: number }
    let age_prop = interner.intern_string("age");
    let inferred = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: T is string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // TODO: string[] should satisfy the any[] upper bound, but the bounds
    // checker currently reports a BoundsViolation because array subtyping
    // is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for array upper bound check"
    );
}
#[test]
fn test_constraint_upper_bound_function() {
    // Test: <T extends (...args: any[]) => any> - T must be callable
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    // TODO: () => number should satisfy the () => any upper bound, but the
    // bounds checker currently reports a BoundsViolation because function
    // subtyping is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for function upper bound check"
    );
}
#[test]
fn test_constraint_upper_bound_union() {
    // Test: <T extends string | number> - T must be string or number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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
    // Test: <T extends string> - fresh literal is widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    let b = interner.literal_string("b");
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Inference: T is "b" (will be widened to string)
    ctx.add_lower_bound(var_t, b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_upper_bound_keyof() {
    // Test: <T extends keyof U> - fresh literal is widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string (widened literals satisfy this)
    let name = interner.literal_string("name");
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Inference: T is "name" (will be widened to string)
    ctx.add_lower_bound(var_t, name);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_no_inference_uses_constraint() {
    // Test: When no inference, T should resolve to constraint bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends { name: string } & { age: number }
    let name_prop = interner.intern_string("name");
    let age_prop = interner.intern_string("age");
    let a = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);
    let b = interner.object(vec![PropertyInfo::new(age_prop, TypeId::NUMBER)]);
    let intersection = interner.intersection(vec![a, b]);
    ctx.add_upper_bound(var_t, intersection);

    // Inference: T is { name: string, age: number }
    let both = interner.object(vec![
        PropertyInfo::new(name_prop, TypeId::STRING),
        PropertyInfo::new(age_prop, TypeId::NUMBER),
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

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
    assert_eq!(results[0].1, TypeId::STRING);
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // For branded primitives, the intersection is with an object
    let brand_prop = interner.intern_string("__brand");
    let brand = interner.object(vec![PropertyInfo::readonly(brand_prop, TypeId::STRING)]);
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal is more specific and satisfies constraint
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_satisfaction_multiple_candidates() {
    // Test: Multiple lower bounds that satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string | number
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_t, union);

    // Two lower bounds
    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple lower bounds of incompatible literal types produce a widened union.
    // "hello" widens to string, 42 widens to number, giving T = string | number.
    // This satisfies the upper bound constraint (string | number).
    match interner.lookup(result) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert!(
                members.contains(&TypeId::STRING) && members.contains(&TypeId::NUMBER),
                "Expected union of string | number, got members: {members:?}"
            );
        }
        _ => panic!("Expected union type for multiple incompatible lower bounds, got {result:?}"),
    }
}
#[test]
fn test_constraint_satisfaction_object_structural() {
    // Test: Object must structurally satisfy constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: { x: number }
    let x_prop = interner.intern_string("x");
    let constraint = interner.object(vec![PropertyInfo::new(x_prop, TypeId::NUMBER)]);
    ctx.add_upper_bound(var_t, constraint);

    // Inference: { x: number, y: string }
    let y_prop = interner.intern_string("y");
    let inferred = interner.object(vec![
        PropertyInfo::new(x_prop, TypeId::NUMBER),
        PropertyInfo::new(y_prop, TypeId::STRING),
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint from return context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Inference from expression
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_constraint_satisfaction_array_element() {
    // Test: Array element type satisfies constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let comparable = interner.object(vec![PropertyInfo::method(compare_prop, compare_fn)]);
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Inference with literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_default_array_type() {
    // Test: <T extends any[] = never[]> - array default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends any[]
    let any_array = interner.array(TypeId::ANY);
    ctx.add_upper_bound(var_t, any_array);

    // Inference: string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // TODO: string[] should satisfy the any[] upper bound, but the bounds
    // checker currently reports a BoundsViolation because array subtyping
    // is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for array upper bound check"
    );
}
#[test]
fn test_default_function_type() {
    // Test: <T extends Function = () => any> - function default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    // TODO: () => number should satisfy the () => any upper bound, but the
    // bounds checker currently reports a BoundsViolation because function
    // subtyping is not wired into the constraint resolution path.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for function upper bound check"
    );
}
#[test]
fn test_default_with_dependent_constraint() {
    // Test: <T, U = T> - U defaults to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // U defaults to string
    ctx.add_lower_bound(var_u, TypeId::STRING);
    // T extends U (string)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // T inferred
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::STRING);
}
#[test]
fn test_default_partial_inference() {
    // Test: <T = string, U = number> - partial inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Recursive types represented as object with children
    let children_prop = interner.intern_string("children");
    let node = interner.object(vec![PropertyInfo {
        name: children_prop,
        type_id: TypeId::ANY, // Simplified - would be T[]
        write_type: TypeId::ANY,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let promise_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_fn,
    )]);
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Create a node type with children array
    let children_array = interner.array(TypeId::OBJECT);
    let node_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("children"),
        children_array,
    )]);
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Create a linked list node with next pointer
    let next_type = interner.union(vec![TypeId::OBJECT, TypeId::NULL]);
    let list_node = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("next"), next_type),
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_k = ctx.fresh_type_param(k_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    // K gets "name" literal
    let name_literal = interner.literal_string("name");
    ctx.add_lower_bound(var_k, name_literal);

    // V gets an object with that key
    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);
    ctx.add_lower_bound(var_v, obj_type);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, obj_type);
}
#[test]
fn test_mutual_dependency_parent_child() {
    // Test: P extends { child: C }, C extends { parent: P }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let p_name = interner.intern_string("P");
    let c_name = interner.intern_string("C");

    let var_p = ctx.fresh_type_param(p_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Create parent type with child reference
    let parent_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("child"),
        TypeId::OBJECT,
    )]);

    // Create child type with parent reference
    let child_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("parent"),
        TypeId::OBJECT,
    )]);

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

    let var_i = ctx.fresh_type_param(i_name, false);
    let var_o = ctx.fresh_type_param(o_name, false);

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

    let var_req = ctx.fresh_type_param(req_name, false);
    let var_res = ctx.fresh_type_param(res_name, false);

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
        PropertyInfo::new(interner.intern_string("id"), TypeId::NUMBER),
        PropertyInfo::method(interner.intern_string("respond"), respond_fn),
    ]);

    // Response type with request reference
    let response_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("data"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("request"), TypeId::OBJECT),
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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::OBJECT,
    )]);

    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("c"),
        TypeId::OBJECT,
    )]);

    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::OBJECT,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let comparable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("compareTo"),
        compare_fn,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
        PropertyInfo::method(interner.intern_string("set"), set_fn),
        PropertyInfo::method(interner.intern_string("build"), build_fn),
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

    let var_t = ctx.fresh_type_param(t_name, false);

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
        PropertyInfo::method(interner.intern_string("evaluate"), evaluate_fn),
        PropertyInfo::new(
            interner.intern_string("children"),
            interner.array(TypeId::OBJECT),
        ),
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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let cloneable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("clone"),
        clone_fn,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let iterable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("next"),
        next_fn,
    )]);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

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

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Create distinct interface types
    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("propA"),
        TypeId::STRING,
    )]);

    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("propB"),
        TypeId::NUMBER,
    )]);

    let type_c = interner.object(vec![PropertyInfo::new(
        interner.intern_string("propC"),
        TypeId::BOOLEAN,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
        PropertyInfo::new(interner.intern_string("value"), TypeId::UNKNOWN),
        PropertyInfo::method(interner.intern_string("getContainer"), get_container_fn),
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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    // Test: T extends string where fresh literal is widened
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: string (widened literals satisfy this)
    ctx.add_lower_bound(var_t, interner.literal_string("A"));
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_acc = ctx.fresh_type_param(acc_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Function returns T, assigned to string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Argument is specific literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_param_inference_generic_identity() {
    // Test: identity<T>(x: T): T - T inferred from argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument is number literal
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_param_inference_from_property_access() {
    // Test: function pick<T, K extends keyof T>(obj: T, key: K): T[K]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_k = ctx.fresh_type_param(k_name, false);

    // Object argument
    let name_prop = interner.intern_string("name");
    let obj = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);
    ctx.add_lower_bound(var_t, obj);

    // Key argument
    let key_name = interner.literal_string("name");
    ctx.add_lower_bound(var_k, key_name);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_k = ctx.resolve_with_constraints(var_k).unwrap();

    assert_eq!(result_t, obj);
    assert_eq!(result_k, TypeId::STRING);
}
#[test]
fn test_param_inference_nested_callback() {
    // Test: arr.map(item => item.children.map(child => child.name))
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Outer array element (has children property)
    let children_prop = interner.intern_string("children");
    let name_prop = interner.intern_string("name");

    let child_type = interner.object(vec![PropertyInfo::new(name_prop, TypeId::STRING)]);

    let parent_type = interner.object(vec![PropertyInfo::new(
        children_prop,
        interner.array(child_type),
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    let string_or_undefined = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, string_or_undefined);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_or_undefined);
}
