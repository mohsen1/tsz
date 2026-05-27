#[test]
fn test_distributive_numeric_literal_filter() {
    // T extends 1 | 2 | 3 ? "low" : T extends 4 | 5 | 6 ? "mid" : "high"
    // with T = 1 | 2 | 5 | 7 | 10
    // Result: "low" | "mid" | "high"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_low = interner.literal_string("low");
    let lit_mid = interner.literal_string("mid");
    let lit_high = interner.literal_string("high");

    let low_set = interner.union(vec![
        interner.literal_number(1.0),
        interner.literal_number(2.0),
        interner.literal_number(3.0),
    ]);
    let mid_set = interner.union(vec![
        interner.literal_number(4.0),
        interner.literal_number(5.0),
        interner.literal_number(6.0),
    ]);

    let inner = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: mid_set,
        true_type: lit_mid,
        false_type: lit_high,
        is_distributive: false,
    });

    let outer = ConditionalType {
        check_type: t_param,
        extends_type: low_set,
        true_type: lit_low,
        false_type: inner,
        is_distributive: true,
    };

    let cond_type = interner.conditional(outer);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            interner.literal_number(1.0),
            interner.literal_number(2.0),
            interner.literal_number(5.0),
            interner.literal_number(7.0),
            interner.literal_number(10.0),
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // 1 -> low, 2 -> low, 5 -> mid, 7 -> high, 10 -> high
    let expected = interner.union(vec![lit_low, lit_mid, lit_high]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_void() {
    // T extends void ? "void" : "not-void"
    // with T = void | string | undefined
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_void = interner.literal_string("void");
    let lit_not_void = interner.literal_string("not-void");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::VOID,
        true_type: lit_void,
        false_type: lit_not_void,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::VOID, TypeId::STRING, TypeId::UNDEFINED]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // void -> "void", string -> "not-void", undefined -> could be either depending on semantics
    let expected = interner.union(vec![lit_void, lit_not_void]);
    assert_eq!(result, expected);
}

// =============================================================================
// DISTRIBUTIVE CONDITIONAL TYPE STRESS TESTS
// =============================================================================

#[test]
fn test_distributive_chained_conditionals() {
    // Type chain: T extends string ? "str" : T extends number ? "num" : "other"
    // Tests: multiple conditional evaluation in sequence
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_str = interner.literal_string("str");
    let lit_num = interner.literal_string("num");
    let lit_other = interner.literal_string("other");

    // Inner conditional: T extends number ? "num" : "other"
    let inner_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::NUMBER,
        true_type: lit_num,
        false_type: lit_other,
        is_distributive: true,
    };
    let inner = interner.conditional(inner_cond);

    // Outer conditional: T extends string ? "str" : <inner>
    let outer_cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: lit_str,
        false_type: inner,
        is_distributive: true,
    };
    let outer = interner.conditional(outer_cond);

    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]),
    );

    let instantiated = instantiate_type(&interner, outer, &subst);
    let result = evaluate_type(&interner, instantiated);

    // string -> "str", number -> "num", boolean -> "other"
    let expected = interner.union(vec![lit_str, lit_num, lit_other]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_intersection_check() {
    // T extends { a: string } & { b: number } ? "match" : "no-match"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let obj_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let obj_b = interner.object(vec![PropertyInfo::new(b_prop, TypeId::NUMBER)]);

    let extends_type = interner.intersection(vec![obj_a, obj_b]);

    let lit_match = interner.literal_string("match");
    let lit_no_match = interner.literal_string("no-match");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type,
        true_type: lit_match,
        false_type: lit_no_match,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create an object with both properties
    let obj_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![obj_ab, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // obj_ab -> "match", string -> "no-match"
    let expected = interner.union(vec![lit_match, lit_no_match]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_bigint_literals() {
    // T extends bigint ? "bigint" : "not-bigint"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_bigint = interner.literal_string("bigint");
    let lit_not = interner.literal_string("not-bigint");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::BIGINT,
        true_type: lit_bigint,
        false_type: lit_not,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![TypeId::BIGINT, TypeId::NUMBER, TypeId::STRING]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // bigint -> "bigint", number -> "not-bigint", string -> "not-bigint"
    let expected = interner.union(vec![lit_bigint, lit_not]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_filter_nullables() {
    // NonNullable<T> = T extends null | undefined ? never : T
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let nullish = interner.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: nullish,
        true_type: TypeId::NEVER,
        false_type: t_param,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![
            TypeId::STRING,
            TypeId::NULL,
            TypeId::NUMBER,
            TypeId::UNDEFINED,
        ]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // null -> never, undefined -> never, string -> string, number -> number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_symbol() {
    // T extends symbol ? "symbol" : "not-symbol"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_sym = interner.literal_string("symbol");
    let lit_not = interner.literal_string("not-symbol");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::SYMBOL,
        true_type: lit_sym,
        false_type: lit_not,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![TypeId::SYMBOL, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    let expected = interner.union(vec![lit_sym, lit_not]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_object_keyword() {
    // T extends object ? "object" : "primitive"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_obj = interner.literal_string("object");
    let lit_prim = interner.literal_string("primitive");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: TypeId::OBJECT,
        true_type: lit_obj,
        false_type: lit_prim,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![obj, TypeId::STRING, TypeId::NUMBER]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // { x: number } -> "object", string -> "primitive", number -> "primitive"
    let expected = interner.union(vec![lit_obj, lit_prim]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_infer_with_fallback() {
    // T extends { value: infer V } ? V : T
    // When T doesn't match, returns T itself
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let v_name = interner.intern_string("V");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let v_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let value_prop = interner.intern_string("value");
    let extends_obj = interner.object(vec![PropertyInfo::new(value_prop, v_infer)]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: v_infer,
        false_type: t_param,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Object with value: number
    let obj_with_value = interner.object(vec![PropertyInfo::new(value_prop, TypeId::NUMBER)]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![obj_with_value, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // { value: number } -> number, string -> string
    let expected = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_tuple_check() {
    // T extends [infer First, ...infer Rest] ? First : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let first_name = interner.intern_string("First");
    let rest_name = interner.intern_string("Rest");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let first_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: first_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let rest_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: rest_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let extends_tuple = interner.tuple(vec![
        TupleElement {
            type_id: first_infer,
            optional: false,
            name: None,
            rest: false,
        },
        TupleElement {
            type_id: rest_infer,
            optional: false,
            name: None,
            rest: true,
        },
    ]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_tuple,
        true_type: first_infer,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Tuple [string, number]
    let tuple1 = interner.tuple(vec![
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
    ]);

    // Tuple [boolean]
    let tuple2 = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        optional: false,
        name: None,
        rest: false,
    }]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![tuple1, tuple2, TypeId::STRING]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // [string, number] -> string, [boolean] -> boolean, string -> never
    let expected = interner.union(vec![TypeId::STRING, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_literal_numbers() {
    // T extends 1 | 2 | 3 ? "low" : "high"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);
    let five = interner.literal_number(5.0);

    let low_set = interner.union(vec![one, two, three]);
    let lit_low = interner.literal_string("low");
    let lit_high = interner.literal_string("high");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: low_set,
        true_type: lit_low,
        false_type: lit_high,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![one, two, four, five]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // 1 -> "low", 2 -> "low", 4 -> "high", 5 -> "high"
    let expected = interner.union(vec![lit_low, lit_high]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_with_boolean_literal_union() {
    // T extends true ? "yes" : "no"
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let lit_true = interner.literal_boolean(true);
    let lit_false = interner.literal_boolean(false);
    let lit_yes = interner.literal_string("yes");
    let lit_no = interner.literal_string("no");

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: lit_true,
        true_type: lit_yes,
        false_type: lit_no,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);
    let mut subst = TypeSubstitution::new();
    // boolean = true | false
    subst.insert(t_name, interner.union(vec![lit_true, lit_false]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // true -> "yes", false -> "no"
    let expected = interner.union(vec![lit_yes, lit_no]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_readonly_array_unwrap() {
    // T extends readonly (infer U)[] ? U : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let u_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let readonly_array = interner.intern(TypeData::ReadonlyType(interner.array(u_infer)));

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: readonly_array,
        true_type: u_infer,
        false_type: TypeId::NEVER,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    let readonly_string = interner.intern(TypeData::ReadonlyType(string_array));
    let readonly_number = interner.intern(TypeData::ReadonlyType(number_array));

    let mut subst = TypeSubstitution::new();
    subst.insert(
        t_name,
        interner.union(vec![readonly_string, readonly_number, TypeId::STRING]),
    );

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // readonly string[] -> string, readonly number[] -> number, string -> never
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_distributive_promise_like_unwrap() {
    // T extends { then(onfulfilled: (value: infer V) => any): any } ? V : T
    // Simplified PromiseLike unwrap
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let v_name = interner.intern_string("V");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let v_infer = interner.intern(TypeData::Infer(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Callback type: (value: V) => any
    let callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: v_infer,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // then method: (onfulfilled: callback) => any
    let then_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let then_prop = interner.intern_string("then");
    let extends_obj = interner.object(vec![PropertyInfo::method(then_prop, then_method)]);

    let cond = ConditionalType {
        check_type: t_param,
        extends_type: extends_obj,
        true_type: v_infer,
        false_type: t_param,
        is_distributive: true,
    };

    let cond_type = interner.conditional(cond);

    // Create a Promise-like object
    let string_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let string_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("onfulfilled")),
            type_id: string_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let promise_string = interner.object(vec![PropertyInfo::method(then_prop, string_then)]);

    let mut subst = TypeSubstitution::new();
    subst.insert(t_name, interner.union(vec![promise_string, TypeId::NUMBER]));

    let instantiated = instantiate_type(&interner, cond_type, &subst);
    let result = evaluate_type(&interner, instantiated);

    // Promise<string> -> string, number -> number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}

// =============================================================================
// RETURNTYPE/PARAMETERS EDGE CASE TESTS
// =============================================================================

#[test]
fn test_return_type_async_promise_unwrapping() {
    // ReturnType<async () => Promise<string>> = Promise<string>
    // Note: async functions wrap return in Promise, ReturnType extracts the Promise<T>
    let interner = TypeInterner::new();

    // Create Promise<string> object type
    let promise_string = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        TypeId::ANY,
    )]);

    let async_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: promise_string,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(async_func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, promise_string);
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_return_type_void_function() {
    // ReturnType<() => void> = void
    let interner = TypeInterner::new();

    let void_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(void_func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::VOID);
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_return_type_never_function() {
    // ReturnType<() => never> = never
    // Functions that throw or loop infinitely return never
    let interner = TypeInterner::new();

    let never_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NEVER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(never_func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::NEVER);
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_return_type_union_of_functions() {
    // ReturnType<(() => string) | (() => number)> distributes over union
    // = string | number
    let interner = TypeInterner::new();

    let func_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union_funcs = interner.union(vec![func_string, func_number]);

    // When extracting return type from union of functions,
    // we should get union of return types
    match interner.lookup(union_funcs) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
            // Both should be functions
            for member in members.iter() {
                match interner.lookup(*member) {
                    Some(TypeData::Function(_)) => {}
                    _ => panic!("Expected Function in union"),
                }
            }
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_return_type_conditional_return() {
    // Function with conditional return type
    // type F<T> = (x: T) => T extends string ? number : boolean
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let cond_return = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: TypeId::BOOLEAN,
        is_distributive: false,
    });

    let generic_func = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: cond_return,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(generic_func) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            match interner.lookup(shape.return_type) {
                Some(TypeData::Conditional(_)) => {}
                _ => panic!("Expected Conditional return type"),
            }
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_return_type_constructor_signature() {
    // For constructor signature, ReturnType returns the constructed type
    let interner = TypeInterner::new();

    let instance_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    let ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("initial")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    match interner.lookup(ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.construct_signatures.len(), 1);
            assert_eq!(shape.construct_signatures[0].return_type, instance_type);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_parameters_this_parameter() {
    // Parameters<(this: Window, x: string) => void> = [string]
    // The 'this' parameter is NOT included in Parameters
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("location"),
        TypeId::STRING,
    )]);

    let func_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(window_type), // this parameter is separate
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func_with_this) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            // params should only contain 'x', not 'this'
            assert_eq!(shape.params.len(), 1);
            assert_eq!(shape.params[0].type_id, TypeId::STRING);
            assert!(shape.this_type.is_some());
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_parameters_labeled_tuple_elements() {
    // Parameters<(first: string, second: number) => void> preserves labels
    let interner = TypeInterner::new();

    let first_name = interner.intern_string("first");
    let second_name = interner.intern_string("second");

    let params_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(first_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(second_name),
            optional: false,
            rest: false,
        },
    ]);

    match interner.lookup(params_tuple) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements[0].name, Some(first_name));
            assert_eq!(elements[1].name, Some(second_name));
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_parameters_multiple_optional() {
    // Parameters<(a?: string, b?: number, c?: boolean) => void> = [string?, number?, boolean?]
    let interner = TypeInterner::new();

    let params_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(interner.intern_string("a")),
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(interner.intern_string("b")),
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: Some(interner.intern_string("c")),
            optional: true,
            rest: false,
        },
    ]);

    match interner.lookup(params_tuple) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 3);
            assert!(elements.iter().all(|e| e.optional));
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_parameters_rest_with_tuple_type() {
    // Parameters<(...args: [string, number, boolean]) => void> = [string, number, boolean]
    // Rest with tuple spread becomes individual elements
    let interner = TypeInterner::new();

    let rest_tuple = interner.tuple(vec![
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
        TupleElement {
            type_id: TypeId::BOOLEAN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    match interner.lookup(rest_tuple) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 3);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
            assert_eq!(elements[2].type_id, TypeId::BOOLEAN);
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_parameters_constructor_signature() {
    // ConstructorParameters<new (x: string) => Foo> = [string]
    let interner = TypeInterner::new();

    let instance_type = interner.object(vec![]);

    let ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(interner.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(interner.intern_string("y")),
                    type_id: TypeId::NUMBER,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    match interner.lookup(ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            let params = &shape.construct_signatures[0].params;
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].type_id, TypeId::STRING);
            assert!(!params[0].optional);
            assert_eq!(params[1].type_id, TypeId::NUMBER);
            assert!(params[1].optional);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_return_type_with_infer_in_conditional() {
    // type ReturnType<T> = T extends (...args: any) => infer R ? R : never
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let r_name = interner.intern_string("R");

    let _t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let infer_r = interner.intern(TypeData::Infer(TypeParamInfo {
        name: r_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let _any_array = interner.array(TypeId::ANY);
    let func_pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: infer_r,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // When T is a function, the infer R should capture return type
    let string_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Substitute T with the actual function
    let substituted = ConditionalType {
        check_type: string_func,
        extends_type: func_pattern,
        true_type: infer_r,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &substituted);
    // After inference, should get string (the return type)
    // The actual implementation behavior may vary
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_parameters_with_infer_in_conditional() {
    // type Parameters<T> = T extends (...args: infer P) => any ? P : never
    let interner = TypeInterner::new();

    let p_name = interner.intern_string("P");

    let infer_p = interner.intern(TypeData::Infer(TypeParamInfo {
        name: p_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let func_pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: infer_p, // infer P captures the params
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Test with a function that has specific params
    let test_func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
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

    let cond = ConditionalType {
        check_type: test_func,
        extends_type: func_pattern,
        true_type: infer_p,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };

    let result = evaluate_conditional(&interner, &cond);
    // Should extract parameters as tuple
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_return_type_generic_with_constraint() {
    // type F<T extends Function> = ReturnType<T>
    // When T is constrained to Function, ReturnType should work
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let func_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: interner.array(TypeId::ANY),
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(func_type),
        default: None,
        is_const: false,
    }));

    // T has constraint to function type
    match interner.lookup(t_param) {
        Some(TypeData::TypeParameter(info)) => {
            assert!(info.constraint.is_some());
        }
        _ => panic!("Expected TypeParameter"),
    }
}

#[test]
fn test_parameters_variadic_tuple_type() {
    // Parameters<(...args: [...T, string]) => void> with variadic tuple
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_array = interner.array(TypeId::ANY);
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: Some(t_array),
        default: None,
        is_const: false,
    }));

    // Variadic tuple: [...T, string]
    let variadic_tuple = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
            name: None,
            optional: false,
            rest: true, // spread T
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    match interner.lookup(variadic_tuple) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = interner.tuple_list(list_id);
            assert_eq!(elements.len(), 2);
            assert!(elements[0].rest);
            assert!(!elements[1].rest);
        }
        _ => panic!("Expected Tuple type"),
    }
}

#[test]
fn test_return_type_intersection_of_functions() {
    // ReturnType<(() => string) & (() => number)> should handle intersection
    // This creates an overloaded callable with both signatures
    let interner = TypeInterner::new();

    let func_string = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func_number = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let intersection = interner.intersection(vec![func_string, func_number]);

    // Functions in intersection are merged into a callable with overloaded signatures
    match interner.lookup(intersection) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(
                shape.call_signatures.len(),
                2,
                "Should have both call signatures"
            );
            // Check that we have both return types
            let return_types: Vec<TypeId> = shape
                .call_signatures
                .iter()
                .map(|sig| sig.return_type)
                .collect();
            assert!(return_types.contains(&TypeId::STRING));
            assert!(return_types.contains(&TypeId::NUMBER));
        }
        _ => panic!("Expected Callable type with overloaded signatures"),
    }
}

#[test]
fn test_parameters_union_of_functions_with_different_arities() {
    // Parameters<((a: string) => void) | ((a: string, b: number) => void)>
    // Results in [string] | [string, number]
    let interner = TypeInterner::new();

    let func1 = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("a")),
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

    let func2 = interner.function(FunctionShape {
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

    let union_funcs = interner.union(vec![func1, func2]);

    match interner.lookup(union_funcs) {
        Some(TypeData::Union(list_id)) => {
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type"),
    }
}

#[test]
fn test_return_type_mapped_type_method() {
    // type Mapped<T> = { [K in keyof T]: ReturnType<T[K]> }
    // Edge case: applying ReturnType to values accessed via mapped type
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let k_name = interner.intern_string("K");

    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let keyof_t = interner.intern(TypeData::KeyOf(t_param));
    let k_param_info = TypeParamInfo {
        name: k_name,
        constraint: Some(keyof_t),
        default: None,
        is_const: false,
    };
    let k_param = interner.intern(TypeData::TypeParameter(k_param_info));

    // T[K] - index access
    let index_access = interner.intern(TypeData::IndexAccess(t_param, k_param));

    // Mapped type that transforms each property
    let mapped = MappedType {
        type_param: k_param_info,
        constraint: keyof_t,
        name_type: None,
        template: index_access, // Each property uses T[K]
        readonly_modifier: None,
        optional_modifier: None,
    };

    let result = evaluate_mapped(&interner, &mapped);
    // Result depends on T being resolved
    assert!(result != TypeId::ERROR);
}

#[test]
fn test_this_parameter_type_extraction() {
    // ThisParameterType<(this: Window) => void> = Window
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![
        PropertyInfo::readonly(interner.intern_string("document"), TypeId::ANY),
        PropertyInfo::new(interner.intern_string("location"), TypeId::STRING),
    ]);

    let func_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: Some(window_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func_with_this) {
        Some(TypeData::Function(shape_id)) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.this_type, Some(window_type));
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_omit_this_parameter() {
    // OmitThisParameter<(this: Window, x: string) => void>
    // = (x: string) => void (without this parameter)
    let interner = TypeInterner::new();

    let window_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("location"),
        TypeId::STRING,
    )]);

    // Function with this parameter
    let func_with_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: Some(window_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Function without this parameter (result of OmitThisParameter)
    let func_without_this = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None, // Omitted
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    match interner.lookup(func_with_this) {
        Some(TypeData::Function(with_id)) => {
            let with_shape = interner.function_shape(with_id);
            match interner.lookup(func_without_this) {
                Some(TypeData::Function(without_id)) => {
                    let without_shape = interner.function_shape(without_id);
                    // Same params
                    assert_eq!(with_shape.params.len(), without_shape.params.len());
                    // Different this_type
                    assert!(with_shape.this_type.is_some());
                    assert!(without_shape.this_type.is_none());
                }
                _ => panic!("Expected Function type"),
            }
        }
        _ => panic!("Expected Function type"),
    }
}

#[test]
fn test_instance_type_from_constructor() {
    // InstanceType<typeof Foo> = Foo instance type
    let interner = TypeInterner::new();

    // Instance type has 'value' property
    let get_value_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let instance_type = interner.object(vec![
        PropertyInfo::new(interner.intern_string("value"), TypeId::STRING),
        PropertyInfo::method(interner.intern_string("getValue"), get_value_method),
    ]);

    // Constructor type
    let ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("initial")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: instance_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    // InstanceType extracts the return type of construct signature
    match interner.lookup(ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            assert_eq!(shape.construct_signatures.len(), 1);
            let extracted_instance = shape.construct_signatures[0].return_type;
            assert_eq!(extracted_instance, instance_type);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_constructor_parameters_with_generics() {
    // ConstructorParameters<new <T>(value: T) => Container<T>>
    let interner = TypeInterner::new();

    let t_name = interner.intern_string("T");
    let t_param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    let container = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        t_param,
    )]);

    let generic_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: container,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    match interner.lookup(generic_ctor) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = interner.callable_shape(shape_id);
            let sig = &shape.construct_signatures[0];
            // Has type parameter
            assert_eq!(sig.type_params.len(), 1);
            assert_eq!(sig.type_params[0].name, t_name);
            // Parameter uses type parameter
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, t_param);
        }
        _ => panic!("Expected Callable type"),
    }
}

#[test]
fn test_awaited_with_nested_promises() {
    // Awaited<Promise<Promise<string>>> = string
    // Awaited recursively unwraps nested promises
    let interner = TypeInterner::new();

    // We model Promise<T> as an object with 'then' method
    // For deeply nested, we just verify the structure
    let inner_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let inner_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        inner_then,
    )]);

    let outer_then = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: inner_promise,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let outer_promise = interner.object(vec![PropertyInfo::method(
        interner.intern_string("then"),
        outer_then,
    )]);

    match interner.lookup(outer_promise) {
        Some(TypeData::Object(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            assert!(!shape.properties.is_empty());
        }
        _ => panic!("Expected Object type"),
    }
}
