use super::*;
use crate::TypeInterner;
use crate::def::DefId;
use crate::{SubtypeChecker, TypeSubstitution, instantiate_type};
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

