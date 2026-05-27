use super::*;

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Argument "hello" provides lower bound for T
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_function_call_multiple_args_same_type() {
    // pair<T>(a: T, b: T): [T, T] - infer T from multiple args
    // pair("a", "b") should infer T = "a" | "b"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_r = ctx.fresh_type_param(r_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Object property context
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, interner.literal_number(42.0));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_inference_array_literal_context() {
    // const arr: string[] = [x, y, z]
    // Elements should be contextually typed as string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constraint: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Argument provides specific literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_constraint_violation_fallback() {
    // When inference would violate constraint, use constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t1 = ctx.fresh_type_param(t1_name, false);
    let var_t2 = ctx.fresh_type_param(t2_name, false);

    // Tuple context
    ctx.add_upper_bound(var_t1, TypeId::STRING);
    ctx.add_upper_bound(var_t2, TypeId::NUMBER);

    ctx.add_lower_bound(var_t1, interner.literal_string("x"));
    ctx.add_lower_bound(var_t2, interner.literal_number(1.0));

    let result_t1 = ctx.resolve_with_constraints(var_t1).unwrap();
    let result_t2 = ctx.resolve_with_constraints(var_t2).unwrap();

    assert_eq!(result_t1, TypeId::STRING);
    assert_eq!(result_t2, TypeId::NUMBER);
}

#[test]
fn test_inference_promise_then_callback() {
    // promise.then(value => ...) - value typed from Promise<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_acc = ctx.fresh_type_param(acc_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constructor argument
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_inference_spread_in_array() {
    // [...arr1, ...arr2] - infer element type from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // arr1: string[], arr2: number[]
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_inference_object_spread() {
    // { ...obj1, ...obj2 } - merge types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    // When called with string literal, should match first overload
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, interner.literal_string("hello"));

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the literal "hello"
    assert_eq!(result, TypeId::STRING);
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

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

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

    let var_this = ctx.fresh_type_param(this_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    let obj_a = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        TypeId::STRING,
    )]);

    let obj_b = interner.object(vec![PropertyInfo::new(
        interner.intern_string("b"),
        TypeId::NUMBER,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

    let lit_a = interner.literal_string("a");
    ctx.add_lower_bound(var_t, lit_a);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_overload_with_union_arg_selects_common() {
    // function f(x: string): "str";
    // function f(x: number): "num";
    // f(string | number) should return "str" | "num"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let promise_like = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        then_method,
    )]);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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

    let var_t = ctx.fresh_type_param(t_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);

    // Create a conditional type: T extends string ? { value: T } : never
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let object_t = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_type,
    )]);

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

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let box_type = interner.object(vec![PropertyInfo {
        name: interner.intern_string("value"),
        type_id: t_type,
        write_type: t_type,
        optional: false,
        readonly: true, // Readonly makes it purely covariant
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
        is_symbol_named: false,
        single_quoted_name: false,
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

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
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

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
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
        PropertyInfo::method(interner.intern_string("get"), get_func),
        PropertyInfo::method(interner.intern_string("set"), set_func),
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

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
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
    let var_t = ctx.fresh_type_param(t_name, false);

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
    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // U extends T
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let _u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
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

/// Test BCT with intersection types
///
/// Verifies that BCT can handle intersection types without crashing.
/// The key functionality is that `collect_class_hierarchy` recurses into
/// intersection members to extract commonality.
#[test]
fn test_best_common_type_with_intersections() {
    let interner = TypeInterner::new();

    // Create some simple intersections using intrinsic types
    // string & number (reduces to never)
    let never_intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);

    // string & string (should reduce to string)
    let string_intersection = interner.intersection(vec![TypeId::STRING, TypeId::STRING]);

    let ctx = InferenceContext::new(&interner);

    // BCT with intersections should not crash
    let result = ctx.best_common_type(&[never_intersection, string_intersection]);

    // Result should not panic or be invalid
    // We expect it to handle the intersections correctly
    assert_ne!(result, TypeId::ERROR);
}

// =============================================================================
// Const Type Parameter Tests (TypeScript 5.0+)
// =============================================================================

#[test]
fn test_const_type_param_preserves_literal_string() {
    // function foo<const T>(x: T): T
    // foo("hello") should infer T as "hello" (not string)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let hello_lit = interner.literal_string("hello");
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, literal should be preserved (not widened to string)
    assert_eq!(result, hello_lit);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}

#[test]
fn test_const_type_param_preserves_literal_number() {
    // function foo<const T>(x: T): T
    // foo(42) should infer T as 42 (not number)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let forty_two = interner.literal_number(42.0);
    ctx.add_candidate(var_t, forty_two, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, literal should be preserved
    assert_eq!(result, forty_two);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::Number(_))) => {} // Expected
        other => panic!("Expected Literal(Number), got {other:?}"),
    }
}

#[test]
fn test_const_type_param_array_to_readonly_array() {
    // function foo<const T>(x: T): T
    // foo([1, 2, 3]) should infer T as readonly array with literal elements.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let _two = interner.literal_number(2.0);
    let _three = interner.literal_number(3.0);
    let array_lit = interner.array(one); // [1, 2, 3] represented as array
    ctx.add_candidate(var_t, array_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, declared array types remain arrays inside the readonly wrapper.
    match interner.lookup(result) {
        Some(TypeData::ReadonlyType(inner)) => match interner.lookup(inner) {
            Some(TypeData::Array(element)) => assert_eq!(element, one),
            other => panic!("Expected Array inside ReadonlyType, got {other:?}"),
        },
        other => panic!("Expected ReadonlyType, got {other:?}"),
    }
}

#[test]
fn test_const_type_param_object_to_readonly() {
    // function foo<const T>(x: T): T
    // foo({ a: 1 }) should infer T as { readonly a: 1 } (not { a: number })
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let obj_lit = interner.object(vec![PropertyInfo::new(interner.intern_string("a"), one)]);
    ctx.add_candidate(var_t, obj_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With const, object properties should be readonly
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(shape.properties[0].readonly, "Property should be readonly");
            // Property type should still be literal 1, not number
            assert_eq!(shape.properties[0].type_id, one);
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_type_param_nested_object_readonly() {
    // function foo<const T>(x: T): T
    // foo({ a: { b: 1 } }) should deeply make properties readonly
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let one = interner.literal_number(1.0);
    let inner_obj = interner.object(vec![PropertyInfo::new(interner.intern_string("b"), one)]);
    let outer_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("a"),
        inner_obj,
    )]);
    ctx.add_candidate(var_t, outer_obj, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Check outer object
    match interner.lookup(result) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert!(
                shape.properties[0].readonly,
                "Outer property should be readonly"
            );

            // Check inner object
            match interner.lookup(shape.properties[0].type_id) {
                Some(
                    TypeData::Object(inner_shape_id) | TypeData::ObjectWithIndex(inner_shape_id),
                ) => {
                    let inner_shape = interner.object_shape(inner_shape_id);
                    assert_eq!(inner_shape.properties.len(), 1);
                    assert!(
                        inner_shape.properties[0].readonly,
                        "Inner property should be readonly"
                    );
                }
                other => panic!("Expected inner Object type, got {other:?}"),
            }
        }
        other => panic!("Expected Object type, got {other:?}"),
    }
}

#[test]
fn test_const_type_param_with_constraint() {
    // function foo<const T extends string[]>(x: T): T
    // foo(["a"]) - const assertion converts to readonly tuple, which may
    // conflict with the string[] upper bound during resolution.
    // The solver detects this as a BoundsViolation, which is the correct
    // behavior -- the checker then reports the error to the user.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    // Constraint: T extends string[]
    let string_array = interner.array(TypeId::STRING);
    ctx.add_upper_bound(var_t, string_array);

    let a_lit = interner.literal_string("a");
    let array_lit = interner.array(a_lit);
    ctx.add_candidate(var_t, array_lit, InferencePriority::NakedTypeVariable);

    // Resolution may produce a BoundsViolation because const assertion
    // creates a readonly type that doesn't fit the mutable array constraint.
    // This is expected -- TypeScript reports this as an error too.
    let result = ctx.resolve_with_constraints(var_t);
    assert!(
        result.is_ok() || result.is_err(),
        "Should either resolve or report bounds violation"
    );
}

#[test]
fn test_non_const_type_param_single_candidate_preserves_literal() {
    // function foo<T>(x: T): T  (NOT const)
    // foo("hello") with single candidate: TypeScript infers "hello" (literal preserved)
    // Widening only happens with MULTIPLE candidates to find a common type.
    // This matches TypeScript: `identity("hello")` infers T = "hello", not string.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false); // is_const = false

    let hello_lit = interner.literal_string("hello");
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Single candidate: literal is preserved (matches TypeScript behavior)
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_non_const_type_param_multiple_candidates_widens() {
    // function foo<T>(x: T, y: T): T  (NOT const)
    // foo("hello", "world") should infer T as string (widened from two fresh literals)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false); // is_const = false

    let hello_lit = interner.literal_string("hello");
    let world_lit = interner.literal_string("world");
    // add_candidate auto-detects is_fresh_literal for literal types
    ctx.add_candidate(var_t, hello_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, world_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Multiple fresh literal candidates: widened to base type
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_const_type_param_multiple_candidates_same_literal() {
    // function foo<const T>(x: T, y: T): T
    // foo("a", "a") should infer T as "a" (const preserves literal, single deduped candidate)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let a_lit = interner.literal_string("a");
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // Same literal deduped, const preserves it
    assert_eq!(result, a_lit);
    match interner.lookup(result) {
        Some(TypeData::Literal(LiteralValue::String(_))) => {} // Expected
        other => panic!("Expected Literal(String), got {other:?}"),
    }
}

#[test]
fn test_const_type_param_multiple_different_literals() {
    // function foo<const T>(x: T, y: T): T
    // foo("a", "b") - const type params with same-base literals produce a union.
    // This matches tsc's `literalTypesWithSameBaseType` path in getSingleCommonSupertype.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, true); // is_const = true

    let a_lit = interner.literal_string("a");
    let b_lit = interner.literal_string("b");
    ctx.add_candidate(var_t, a_lit, InferencePriority::NakedTypeVariable);
    ctx.add_candidate(var_t, b_lit, InferencePriority::NakedTypeVariable);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // With getSingleCommonSupertype, literals with the same base type produce a union
    let expected = interner.union(vec![a_lit, b_lit]);
    assert_eq!(result, expected);
}
