/// Creates a `TypeEnvironment` with a mock Array<T> interface for testing.
/// The interface includes: length, map, at, entries, and reduce.
fn make_array_test_env(
    interner: &TypeInterner,
) -> (
    crate::relations::subtype::TypeEnvironment,
    crate::types::TypeParamInfo,
) {
    use crate::relations::subtype::TypeEnvironment;
    use crate::types::TypeParamInfo;

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // length: number
    let length_prop = PropertyInfo::readonly(interner.intern_string("length"), TypeId::NUMBER);

    // map<U>(callbackfn: (value: T, index: number, array: T[]) => U, thisArg?: any): U[]
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let map_callback = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("index")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let map_func = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("callbackfn")),
                type_id: map_callback,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("thisArg")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            },
        ],
        return_type: interner.array(u_type),
        type_params: vec![u_param],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // at(index: number): T | undefined
    let at_func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("index")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        return_type: interner.union(vec![t_type, TypeId::UNDEFINED]),
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // entries(): Array<[number, T]>
    let entry_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let entries_func = interner.function(FunctionShape {
        params: vec![],
        return_type: interner.array(entry_tuple),
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    // reduce(callbackfn: (prev: T, curr: T, idx: number, arr: T[]) => T): T
    use crate::types::CallSignature;
    let reduce_cb_1 = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("previousValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentIndex")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: t_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let reduce_sig_1 = CallSignature {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("callbackfn")),
            type_id: reduce_cb_1,
            optional: false,
            rest: false,
        }],
        return_type: t_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_method: true,
    };
    // reduce<U>(callbackfn: (prev: U, curr: T, idx: number, arr: T[]) => U, initialValue: U): U
    let reduce_cb_2 = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("previousValue")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentValue")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("currentIndex")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("array")),
                type_id: interner.array(t_type),
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let reduce_sig_2 = CallSignature {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("callbackfn")),
                type_id: reduce_cb_2,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("initialValue")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        return_type: u_type,
        type_params: vec![u_param],
        this_type: None,
        type_predicate: None,
        is_method: true,
    };
    let reduce_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![reduce_sig_1, reduce_sig_2],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
    });

    let array_interface = interner.object(vec![
        length_prop,
        PropertyInfo::method(interner.intern_string("map"), map_func),
        PropertyInfo::method(interner.intern_string("at"), at_func),
        PropertyInfo::method(interner.intern_string("entries"), entries_func),
        PropertyInfo::method(interner.intern_string("reduce"), reduce_callable),
    ]);

    // Set array base type on the interner so PropertyAccessEvaluator can find it
    interner.set_array_base_type(array_interface, vec![t_param]);

    let mut env = TypeEnvironment::new();
    env.set_array_base_type(array_interface, vec![t_param]);

    (env, t_param)
}

#[test]
fn test_property_access_readonly_array() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::STRING);
    let readonly_array = interner.intern(TypeData::ReadonlyType(array));

    let result = evaluator.resolve_property_access(readonly_array, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Fixed-length tuple [number, string] → .length should be literal 2
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let expected_literal = interner.literal_number(2.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_empty_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Empty tuple [] → .length should be literal 0
    let tuple = interner.tuple(vec![]);
    let expected_literal = interner.literal_number(0.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_single_element_tuple_length() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Single element tuple [number] → .length should be literal 1
    let tuple = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);
    let expected_literal = interner.literal_number(1.0);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, expected_literal),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_length_stays_number() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Array type number[] → .length should remain `number`, not a literal
    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_tuple_with_rest_length_stays_number() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // Tuple with array rest element [number, ...string[]] → variable length → `number`
    let rest_array = interner.array(TypeId::STRING);
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let result = evaluator.resolve_property_access(tuple, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_map_signature() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "map");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.type_params.len(), 1, "map should have 1 type param U");
                assert_eq!(func.params.len(), 2, "map should have 2 params");
                let u_param = &func.type_params[0];
                let u_type = interner.intern(TypeData::TypeParameter(*u_param));
                let expected_return = interner.array(u_type);
                assert_eq!(func.return_type, expected_return, "map should return U[]");

                let callback_type = func.params[0].type_id;
                match interner.lookup(callback_type) {
                    Some(TypeData::Function(cb_id)) => {
                        let callback = interner.function_shape(cb_id);
                        assert_eq!(callback.return_type, u_type);
                        assert_eq!(callback.params[0].type_id, TypeId::NUMBER); // T=number
                        assert_eq!(callback.params[1].type_id, TypeId::NUMBER); // index
                        assert_eq!(callback.params[2].type_id, array); // array: number[]
                    }
                    other => panic!("Expected callback function, got {other:?}"),
                }
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_at_returns_optional_element() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_property_access(array, "at");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
                assert_eq!(func.return_type, expected);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_entries_returns_tuple_array() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::BOOLEAN);
    let result = evaluator.resolve_property_access(array, "entries");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                let Some(TypeData::Array(return_elem)) = interner.lookup(func.return_type) else {
                    panic!("Expected array return type");
                };
                let Some(TypeData::Tuple(tuple_id)) = interner.lookup(return_elem) else {
                    panic!("Expected tuple element type");
                };
                let tuple = interner.tuple_list(tuple_id);
                assert_eq!(tuple.len(), 2);
                assert_eq!(tuple[0].type_id, TypeId::NUMBER);
                assert_eq!(tuple[1].type_id, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_indexof_preserves_nullable_element_type() {
    let interner = TypeInterner::new();
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let index_of = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("searchElement")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("fromIndex")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_interface = interner.object(vec![PropertyInfo::method(
        interner.intern_string("indexOf"),
        index_of,
    )]);
    interner.set_array_base_type(array_interface, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let element = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED, TypeId::NULL]);
    let array = interner.array(element);

    let result = evaluator.resolve_property_access(array, "indexOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(
                    func.params[0].type_id, element,
                    "indexOf should use the full nullable element type"
                );
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_callable_array_indexof_preserves_nullable_element_type() {
    let interner = TypeInterner::new();
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let index_of = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("searchElement")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("fromIndex")),
                type_id: TypeId::NUMBER,
                optional: true,
                rest: false,
            },
        ],
        return_type: TypeId::NUMBER,
        type_params: vec![],
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let array_callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: Vec::new(),
        properties: vec![PropertyInfo::method(
            interner.intern_string("indexOf"),
            index_of,
        )],
        string_index: None,
        number_index: None,
    });
    interner.set_array_base_type(array_callable, vec![t_param]);

    let evaluator = PropertyAccessEvaluator::new(&interner);
    let element = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED, TypeId::NULL]);
    let array = interner.array(element);

    let result = evaluator.resolve_property_access(array, "indexOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(
                    func.params[0].type_id, element,
                    "callable-backed Array#indexOf should keep the full nullable element type"
                );
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_array_reduce_callable() {
    let interner = TypeInterner::new();
    let (_env, _) = make_array_test_env(&interner);
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let array = interner.array(TypeId::STRING);
    let result = evaluator.resolve_property_access(array, "reduce");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Callable(callable_id)) => {
                let callable = interner.callable_shape(callable_id);
                assert_eq!(callable.call_signatures.len(), 2);
                assert_eq!(callable.call_signatures[0].return_type, TypeId::STRING);
                let generic_sig = &callable.call_signatures[1];
                assert_eq!(generic_sig.type_params.len(), 1);
                let u_type = interner.intern(TypeData::TypeParameter(generic_sig.type_params[0]));
                assert_eq!(generic_sig.return_type, u_type);
            }
            other => panic!("Expected callable, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_void() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::VOID, "x");
    match result {
        PropertyAccessResult::PropertyNotFound { .. } => {
            // void has no properties; solver returns PropertyNotFound
        }
        _ => panic!("Expected PropertyNotFound, got {result:?}"),
    }
}

#[test]
fn test_property_access_index_signature_no_unchecked() {
    let interner = TypeInterner::new();
    let mut evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluator.resolve_property_access(obj, "anything");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            assert_eq!(type_id, TypeId::NUMBER);
            assert!(from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    evaluator.set_no_unchecked_indexed_access(true);

    let result = evaluator.resolve_property_access(obj, "anything");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_object_with_index_optional_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo::opt(
            interner.intern_string("x"),
            TypeId::NUMBER,
        )],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::BOOLEAN,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success {
            type_id,
            write_type: _,
            from_index_signature,
        } => {
            let expected = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);
            assert_eq!(type_id, expected);
            assert!(!from_index_signature);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_string() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_number_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "toFixed");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::STRING);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_boolean_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BOOLEAN, "valueOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_bigint_method() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::BIGINT, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::STRING);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_object_methods_on_primitives() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::STRING, "hasOwnProperty");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::NUMBER, "isPrototypeOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::BOOLEAN, "propertyIsEnumerable");
    match result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let func = interner.function_shape(func_id);
                assert_eq!(func.return_type, TypeId::BOOLEAN);
            }
            other => panic!("Expected function, got {other:?}"),
        },
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_reuses_context_across_name_lengths() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let long_name = "hasOwnProperty";
    let short_name = "length";

    let long_result = evaluator.resolve_property_access(TypeId::STRING, long_name);
    match long_result {
        PropertyAccessResult::Success { type_id, .. } => match interner.lookup(type_id) {
            Some(TypeData::Function(_)) => {}
            other => panic!("Expected function for long name, got {other:?}"),
        },
        _ => panic!("Expected success for long name, got {long_result:?}"),
    }

    let short_result = evaluator.resolve_property_access(TypeId::STRING, short_name);
    match short_result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(type_id, TypeId::NUMBER),
        _ => panic!("Expected success for short name, got {short_result:?}"),
    }
}

#[test]
fn test_property_access_primitive_constructor_value() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "constructor");
    match result {
        PropertyAccessResult::Success { type_id, .. } => assert_eq!(type_id, TypeId::FUNCTION),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_symbol_primitive_methods_use_apparent_return_types() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected symbol.toString to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::SYMBOL, "valueOf");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected symbol.valueOf to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::SYMBOL);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_template_literal() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = evaluator.resolve_property_access(template, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_literal_string_length() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let literal = interner.literal_string("hello");
    let result = evaluator.resolve_property_access(literal, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_binary_op_addition() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // number + number = number
    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::NUMBER, "+");
    match result {
        BinaryOpResult::Success(t) => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    // string + number = string
    let result = evaluator.evaluate(TypeId::STRING, TypeId::NUMBER, "+");
    match result {
        BinaryOpResult::Success(t) => assert_eq!(t, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_binary_op_logical() {
    let interner = TypeInterner::new();
    let evaluator = BinaryOpEvaluator::new(&interner);

    // number && string = 0 | string (definitely-falsy part of number is literal 0)
    let result = evaluator.evaluate(TypeId::NUMBER, TypeId::STRING, "&&");
    match result {
        BinaryOpResult::Success(t) => {
            // Should be a union type with 0 (literal) and string
            let key = interner.lookup(t).unwrap();
            match key {
                TypeData::Union(members) => {
                    let members = interner.type_list(members);
                    assert_eq!(members.len(), 2, "Expected 2 members, got {members:?}");
                    assert!(members.contains(&TypeId::STRING));
                    // The other member should be a number literal 0
                    let zero_type = members.iter().find(|&&m| m != TypeId::STRING).unwrap();
                    match interner.lookup(*zero_type) {
                        Some(TypeData::Literal(LiteralValue::Number(n))) => {
                            assert_eq!(n.0, 0.0, "Expected 0, got {}", n.0);
                        }
                        other => panic!("Expected number literal 0, got {other:?}"),
                    }
                }
                _ => panic!("Expected union, got {key:?}"),
            }
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_generic_function_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function identity<T>(x: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
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

    // Call identity(42) -> should infer T = number
    let result = evaluator.resolve_call(func, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_generic_function_with_string() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function identity<T>(x: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
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

    // Call identity("hello") -> should infer T = string
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_generic_argument_type_mismatch_with_default() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(TypeId::NUMBER),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let param_type = interner.union(vec![t_type, TypeId::NUMBER]);

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
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
    });

    // Call foo<T = number>(x: T | number) with "hello".
    // In TypeScript, defaults are fallbacks when no inference candidates exist,
    // not constraints that prevent inference. T is inferred as string from the
    // argument, so x: string | number, and string is assignable → success.
    let result = evaluator.resolve_call(func, &[TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected Success with T=string, got {result:?}"),
    }
}

#[test]
fn test_call_generic_direct_param_candidate_keeps_first_for_conflicting_literals() {
    // In tsc, f<T>(x: T, y: T) called with f(1, "") infers T as a union 1 | ""
    // (multiple inference candidates are unioned). The call succeeds.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let one = interner.literal_number(1.0);
    let two = interner.literal_string("");

    let result = evaluator.resolve_call(func, &[one, two]);
    // tsc's getSingleCommonSupertype uses first-wins for fresh literals:
    // T = 1 (widened to number), then "" is checked against number → TS2345.
    // So the call FAILS with ArgumentTypeMismatch, not Success.
    assert!(
        matches!(result, CallResult::ArgumentTypeMismatch { .. }),
        "Expected ArgumentTypeMismatch (tsc's first-wins), got {result:?}"
    );
}

#[test]
fn test_call_generic_argument_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
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

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            actual,
            ..
        } => {
            assert_eq!(expected_min, 1);
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_generic_rest_tuple_constraint_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_constraint = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(tuple_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(expected_max, Some(2));
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_generic_default_rest_tuple_count_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_default = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(tuple_default),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 2);
            assert_eq!(expected_max, Some(2));
            assert_eq!(actual, 0);
        }
        _ => panic!("Expected ArgumentCountMismatch, got {result:?}"),
    }
}

/// Regression test: call<TS extends unknown[]>(handler: (...args: TS) => void, ...args: TS)
/// with too many args should emit TS2554. The handler's params infer TS = [number, number],
/// so the function expects 3 args total (handler + 2 numbers). Passing 8 args should fail.
/// This tests that `rest_tuple_inference` is skipped when the type variable also appears
/// in another parameter (the handler), preventing the rest args from overriding the
/// handler-inferred tuple type.
#[test]
fn test_call_generic_rest_excess_args_detected_when_shared_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // TS extends unknown[]
    let unknown_array = interner.array(TypeId::UNKNOWN);
    let ts_param = TypeParamInfo {
        name: interner.intern_string("TS"),
        constraint: Some(unknown_array),
        default: None,
        is_const: false,
    };
    let ts_type = interner.intern(TypeData::TypeParameter(ts_param));

    // handler: (...args: TS) => void
    let handler_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: ts_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // call<TS extends unknown[]>(handler: (...args: TS) => void, ...args: TS): void
    let call_fn = interner.function(FunctionShape {
        type_params: vec![ts_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("handler")),
                type_id: handler_fn,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("args")),
                type_id: ts_type,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // The handler callback: (x: number, y: number) => number
    let handler_arg = interner.function(FunctionShape {
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
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
        return_type: TypeId::NUMBER,
        type_params: vec![],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with 8 args: call(handler, 1, 2, 3, 4, 5, 6, 7)
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    let three = interner.literal_number(3.0);
    let four = interner.literal_number(4.0);
    let five = interner.literal_number(5.0);
    let six = interner.literal_number(6.0);
    let seven = interner.literal_number(7.0);
    let result = evaluator.resolve_call(
        call_fn,
        &[handler_arg, one, two, three, four, five, six, seven],
    );

    match result {
        CallResult::ArgumentCountMismatch {
            expected_min,
            expected_max,
            actual,
        } => {
            assert_eq!(expected_min, 3, "handler + 2 tuple elements");
            assert_eq!(expected_max, Some(3), "fixed-length tuple [number, number]");
            assert_eq!(actual, 8, "handler + 7 number args");
        }
        _ => panic!("Expected ArgumentCountMismatch for excess args, got {result:?}"),
    }
}

#[test]
fn test_call_generic_default_rest_tuple_optional_allows_empty() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let tuple_default = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: true,
        rest: false,
    }]);
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: Some(tuple_default),
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: t_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, tuple_default),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_generic_argument_type_mismatch_non_generic_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    // function foo<T>(x: number, y: T): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, TypeId::NUMBER);
            assert_eq!(actual, TypeId::STRING);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_call_generic_callable_signature() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: vec![t_param],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_type,
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
        ..Default::default()
    });

    let result = evaluator.resolve_call(callable, &[TypeId::NUMBER]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_call_generic_array_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Create type parameter T
    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let array_t = interner.array(t_type);

    // function first<T>(arr: T[]): T
    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arr")),
            type_id: array_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call first(number[]) -> should infer T = number
    let number_array = interner.array(TypeId::NUMBER);
    let result = evaluator.resolve_call(func, &[number_array]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_infer_call_signature_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let sig = CallSignature {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_method: false,
    };

    let result = infer_call_signature(&interner, &mut subtype, &sig, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_function_identity() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let func = FunctionShape {
        type_params: vec![t_param],
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
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_generic_call_resets_fixed_union_member_cache() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let identity = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(t_type), ParamInfo::unnamed(t_type)],
        this_type: None,
        return_type: t_type,
        type_params: vec![t_param],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    evaluator
        .constraint_fixed_union_members
        .borrow_mut()
        .insert(TypeId::STRING, rustc_hash::FxHashSet::default());

    let result = evaluator.resolve_call(identity, &[TypeId::STRING, TypeId::STRING]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::STRING),
        _ => panic!("Expected successful generic inference, got {result:?}"),
    }

    assert!(evaluator.constraint_fixed_union_members.borrow().is_empty());
}

#[test]
fn test_infer_generic_function_identity_preserves_unconstrained_scalar_literal() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let hello = interner.literal_string("hello");
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &make_identity_shape(&interner, "T", "x"),
        &[hello],
    );
    assert_eq!(result, hello);

    // Different param name proves the rule is structural, not name-specific.
    let world = interner.literal_string("world");
    let result2 = infer_generic_function(
        &interner,
        &mut subtype,
        &make_identity_shape(&interner, "U", "value"),
        &[world],
    );
    assert_eq!(result2, world);
}
