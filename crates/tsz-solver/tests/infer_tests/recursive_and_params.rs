use super::*;

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

#[test]
fn test_param_inference_constrained_to_subset() {
    // Test: function f<T extends string>(x: T) - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var_t, TypeId::STRING);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_param_inference_from_tuple_destructure() {
    // Test: const [a, b] = f<[T, U]>([1, "hello"])
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument provides lower bound
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    // Return context provides upper bound (widened to string)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_param_inference_void_callback() {
    // Test: arr.forEach(x => console.log(x)) - callback returns void
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let comparable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("compareTo"),
        compare_to_fn,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
        PropertyInfo::method(interner.intern_string("build"), build_fn),
        PropertyInfo::method(interner.intern_string("set"), set_fn),
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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Children is array of self-type
    let children_array = interner.array(TypeId::OBJECT);

    let tree_node_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::UNKNOWN),
        PropertyInfo::new(interner.intern_string("children"), children_array),
        PropertyInfo::opt(interner.intern_string("parent"), TypeId::OBJECT),
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

    let var_t = ctx.fresh_type_param(t_name, false);

    let clone_fn = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let cloneable_type = interner.object(vec![PropertyInfo::method(
        interner.intern_string("clone"),
        clone_fn,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
        PropertyInfo::method(interner.intern_string("compareTo"), compare_fn),
        PropertyInfo::method(interner.intern_string("serialize"), serialize_fn),
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let type_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let type_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

    ctx.add_lower_bound(var_t, type_a);
    ctx.add_lower_bound(var_u, type_b);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);
    ctx.add_upper_bound(var_u, TypeId::OBJECT);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_extends_clause_with_keyof() {
    // Test: T extends string - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: string (widened literal satisfies this)
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("a"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_extends_clause_with_mapped_type_key() {
    // Test: K extends string - fresh literal widened to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");

    let var_k = ctx.fresh_type_param(k_name, false);

    // Upper bound: string (widened literal satisfies this)
    ctx.add_upper_bound(var_k, TypeId::STRING);
    ctx.add_lower_bound(var_k, interner.literal_string("name"));

    let result = ctx.resolve_with_constraints(var_k).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_extends_clause_conditional_constraint() {
    // Test: T extends U ? X : Y pattern constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_extends_clause_array_constraint() {
    // Test: T extends Array<infer U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}
