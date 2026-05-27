/// When a fresh object literal `{ x: 1 }` is passed to `identity<T>(x: T): T` with
/// unconstrained T, tsc infers T = `{ x: number }` (widened), not `{ x: 1 }`.
/// This mirrors tsc's `getWidenedType` behavior in inference resolution.
#[test]
fn test_identity_widens_fresh_object_literal_properties_for_unconstrained_t() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    // Case 1: { x: 1 } passed to identity<T>(x: T): T → T = { x: number }
    let prop_x = interner.intern_string("x");
    let lit_1 = interner.literal_number(1.0);
    let obj_x1 = interner.object_fresh(vec![PropertyInfo::new(prop_x, lit_1)]);
    let ret = infer_generic_function(
        &interner,
        &mut checker,
        &make_identity_shape(&interner, "T", "x"),
        &[obj_x1],
    );
    let shape = match interner.lookup(ret) {
        Some(TypeData::Object(s)) | Some(TypeData::ObjectWithIndex(s)) => interner.object_shape(s),
        other => panic!("Expected object return type, got {other:?}"),
    };
    assert_eq!(shape.properties.len(), 1);
    assert_eq!(
        shape.properties[0].type_id,
        TypeId::NUMBER,
        "Property 'x' should be widened to number"
    );
    assert!(
        !shape
            .flags
            .contains(crate::types::ObjectFlags::FRESH_LITERAL),
        "FRESH_LITERAL should be stripped from widened result"
    );

    // Case 2: { alpha: false } passed to wrap<U>(v: U): U → U = { alpha: boolean }
    // Different param name, property name, and value type — proves rule is structural.
    let prop_alpha = interner.intern_string("alpha");
    let obj_alpha_false =
        interner.object_fresh(vec![PropertyInfo::new(prop_alpha, TypeId::BOOLEAN_FALSE)]);
    let ret2 = infer_generic_function(
        &interner,
        &mut checker,
        &make_identity_shape(&interner, "U", "v"),
        &[obj_alpha_false],
    );
    let shape2 = match interner.lookup(ret2) {
        Some(TypeData::Object(s)) | Some(TypeData::ObjectWithIndex(s)) => interner.object_shape(s),
        other => panic!("Expected object return type, got {other:?}"),
    };
    assert_eq!(
        shape2.properties[0].type_id,
        TypeId::BOOLEAN,
        "Property 'alpha' should be widened to boolean"
    );

    // Case 3: multi-property object { name: "hi", count: 42 } → { name: string, count: number }
    let prop_name = interner.intern_string("name");
    let prop_count = interner.intern_string("count");
    let lit_hi = interner.literal_string("hi");
    let lit_42 = interner.literal_number(42.0);
    let obj_multi = interner.object_fresh(vec![
        PropertyInfo::new(prop_name, lit_hi),
        PropertyInfo::new(prop_count, lit_42),
    ]);
    let ret3 = infer_generic_function(
        &interner,
        &mut checker,
        &make_identity_shape(&interner, "K", "input"),
        &[obj_multi],
    );
    let shape3 = match interner.lookup(ret3) {
        Some(TypeData::Object(s)) | Some(TypeData::ObjectWithIndex(s)) => interner.object_shape(s),
        other => panic!("Expected object return type, got {other:?}"),
    };
    let by_name: std::collections::HashMap<_, _> = shape3
        .properties
        .iter()
        .map(|p| (interner.resolve_atom(p.name), p.type_id))
        .collect();
    assert_eq!(
        by_name["name"],
        TypeId::STRING,
        "Property 'name' should be widened to string"
    );
    assert_eq!(
        by_name["count"],
        TypeId::NUMBER,
        "Property 'count' should be widened to number"
    );
}

/// Scalar literals stay literal for unconstrained T (tsc: `identity(1)` -> T = 1).
#[test]
fn test_identity_preserves_scalar_literals_for_unconstrained_t() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    for (param_name, arg_name) in [("T", "x"), ("U", "value"), ("R", "item")] {
        let func = make_identity_shape(&interner, param_name, arg_name);
        let n = interner.literal_number(5.0);
        let result = infer_generic_function(&interner, &mut checker, &func, &[n]);
        assert_eq!(
            result, n,
            "{param_name}: literal number 5 should stay literal"
        );
    }
}

#[test]
fn test_infer_generic_function_identity_preserves_const_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: true,
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

    let hello = interner.literal_string("hello");
    let result = infer_generic_function(&interner, &mut subtype, &func, &[hello]);
    assert_eq!(result, hello);
}

#[test]
fn test_infer_generic_function_this_type_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let param_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: param_func,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_func]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_callable_param_from_function() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(t_type),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callable_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_func = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_func]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_function_param_from_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let function_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: function_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("arg")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId::NUMBER),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_function_param_from_overloaded_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let function_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
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

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: function_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("value")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: Vec::new(),
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
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_generic_function_from_union_call_or_construct_argument() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);

    let value_type = interner.union(vec![
        interner.literal_string("A"),
        interner.literal_string("B"),
    ]);
    let exact_props = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        value_type,
    )]);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let target_call = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(t_type)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target_construct = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(t_type)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("type")),
                type_id: interner.union(vec![target_call, target_construct, TypeId::STRING]),
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("props")),
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
    };

    let source_call = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(exact_props)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let source_construct = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(exact_props)],
        this_type: None,
        return_type: TypeId::ANY,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let jsx_element_constructor = interner.union(vec![source_call, source_construct]);

    let result = infer_generic_function(
        &interner,
        &mut checker,
        &func,
        &[jsx_element_constructor, exact_props],
    );
    assert_eq!(result, exact_props);
}

#[test]
fn test_infer_generic_final_argument_check_uses_non_strict_assignability() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callback_param_type = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
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

    let animal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("name"),
        TypeId::STRING,
    )]);
    let dog = interner.object(vec![
        PropertyInfo::new(interner.intern_string("name"), TypeId::STRING),
        PropertyInfo::new(interner.intern_string("breed"), TypeId::STRING),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("cb")),
                type_id: callback_param_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("value")),
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
    };

    let callback_arg = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: animal,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callback_arg, dog]);
    assert_eq!(result, dog);
}

#[test]
fn test_infer_generic_object_with_contextual_callbacks_prefers_schema_property_type() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let query = interner.intern_string("query");
    let body = interner.intern_string("body");
    let pre = interner.intern_string("pre");
    let schema = interner.intern_string("schema");
    let handle = interner.intern_string("handle");
    let req_arg = interner.intern_string("req");
    let pre_arg = interner.intern_string("a");

    let schema_constraint = interner.object(vec![
        PropertyInfo::opt(query, TypeId::UNKNOWN),
        PropertyInfo::opt(body, TypeId::UNKNOWN),
    ]);
    let schema_arg = interner.object(vec![PropertyInfo::new(
        query,
        interner.literal_string("query-string"),
    )]);

    let t_param = TypeParamInfo {
        name: interner.intern_string("TSchema"),
        constraint: Some(schema_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let pre_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(pre_arg),
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

    let pre_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(pre_arg),
            type_id: schema_constraint,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let request_query = interner.index_access(t_type, interner.literal_string("query"));
    let handle_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(req_arg),
            type_id: request_query,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(req_arg),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let shape_param = interner.object(vec![
        PropertyInfo::new(pre, pre_target),
        PropertyInfo::new(schema, t_type),
        PropertyInfo::new(handle, handle_target),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("options")),
            type_id: shape_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.object(vec![
        PropertyInfo::new(pre, pre_source),
        PropertyInfo::new(schema, schema_arg),
        PropertyInfo::new(handle, handle_source),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg]);
    assert_eq!(result, schema_arg);
}

#[test]
fn test_infer_generic_mixed_object_argument_infers_from_non_contextual_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let query = interner.intern_string("query");
    let pre = interner.intern_string("pre");
    let schema = interner.intern_string("schema");
    let handle = interner.intern_string("handle");

    let schema_constraint = interner.object(vec![PropertyInfo::new(query, TypeId::STRING)]);
    let schema_arg = interner.object(vec![PropertyInfo::new(
        query,
        interner.literal_string("query-string"),
    )]);

    let t_param = TypeParamInfo {
        name: interner.intern_string("TSchema"),
        constraint: Some(schema_constraint),
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let pre_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
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

    let pre_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: schema_constraint,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: interner.index_access(t_type, interner.literal_string("query")),
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let handle_source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: TypeId::ANY,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let shape_param = interner.object(vec![
        PropertyInfo::new(pre, pre_target),
        PropertyInfo::new(schema, t_type),
        PropertyInfo::new(handle, handle_target),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("route_args")),
            type_id: shape_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.object(vec![
        PropertyInfo::new(pre, pre_source),
        PropertyInfo::new(schema, schema_arg),
        PropertyInfo::new(handle, handle_source),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg]);
    // The inference may instantiate the constraint, producing a structurally
    // different but semantically valid return type. Verify the result is an
    // object type (inference succeeded, not ERROR/UNKNOWN).
    let is_objectish =
        |ty| ty == schema_arg || matches!(interner.lookup(ty), Some(TypeData::Object(_)));
    assert!(
        is_objectish(result)
            || matches!(
                interner.lookup(result),
                Some(TypeData::Union(members))
                    if interner.type_list(members).iter().copied().all(is_objectish)
            ),
        "Expected inference to return an object type, got {result:?} = {:?}",
        interner.lookup(result),
    );
}

#[test]
fn test_infer_generic_callable_param_from_callable() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let callable_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(t_type),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callable_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let callable_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId::NUMBER),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[callable_arg]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_construct_signature_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let ctor_param = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: t_type,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("ctor")),
            type_id: ctor_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let ctor_arg = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("value")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        properties: Vec::new(),
        ..Default::default()
    });

    let result = infer_generic_function(&interner, &mut subtype, &func, &[ctor_arg]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_keyof_param_from_keyof_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let keyof_param = interner.intern(TypeData::KeyOf(t_type));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("key")),
            type_id: keyof_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::NUMBER,
    )]);
    let arg_keyof = interner.intern(TypeData::KeyOf(obj));

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_keyof]);
    assert_eq!(result, obj);
}

#[test]
fn test_infer_generic_index_access_param_from_object_property_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let key_x = interner.literal_string("x");
    let obj = interner.object(vec![PropertyInfo::new(interner.intern_string("x"), t_type)]);
    let index_access_param = interner.intern(TypeData::IndexAccess(obj, key_x));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: index_access_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_template_literal_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let template_param = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(t_type),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: template_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("suffix")),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_template]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_generic_conditional_param_from_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let conditional = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let conditional_type = interner.conditional(conditional);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: conditional_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_conditional_param_with_check_placeholder_from_branch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let conditional = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::NEVER,
        false_type: t_type,
        is_distributive: false,
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: conditional,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_mapped_param_from_object_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let key_param = TypeParamInfo {
        name: interner.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let key_x = interner.literal_string("x");
    let key_y = interner.literal_string("y");
    let keys = interner.union(vec![key_x, key_y]);

    let mapped = MappedType {
        type_param: key_param,
        constraint: keys,
        name_type: None,
        template: t_type,
        readonly_modifier: None,
        optional_modifier: None,
    };
    let mapped_type = interner.mapped(mapped);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("values")),
            type_id: mapped_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg_object = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::NUMBER),
    ]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg_object]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_array_map() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let array_t = interner.array(t_type);
    let array_u = interner.array(u_type);

    let callback_param = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
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

    let map_func = FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("arr")),
                type_id: array_t,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("callback")),
                type_id: callback_param,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: array_u,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let number_array = interner.array(TypeId::NUMBER);
    let callback_arg = interner.function(FunctionShape {
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

    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &map_func,
        &[number_array, callback_arg],
    );
    let expected = interner.array(TypeId::STRING);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_generic_array_param_from_tuple_arg() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let array_t = interner.array(t_type);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("values")),
            type_id: array_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let tuple_arg = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::NUMBER,
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

    let result = infer_generic_function(&interner, &mut subtype, &func, &[tuple_arg]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_readonly_array_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let readonly_array_t = interner.intern(TypeData::ReadonlyType(interner.array(t_type)));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("items")),
            type_id: readonly_array_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let readonly_number_array =
        interner.intern(TypeData::ReadonlyType(interner.array(TypeId::NUMBER)));
    let result = infer_generic_function(&interner, &mut subtype, &func, &[readonly_number_array]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_readonly_tuple_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));
    let readonly_tuple_t =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        }])));

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("pair")),
            type_id: readonly_tuple_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let readonly_tuple_number =
        interner.intern(TypeData::ReadonlyType(interner.tuple(vec![TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        }])));
    let result = infer_generic_function(&interner, &mut subtype, &func, &[readonly_tuple_number]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_generic_constructor_instantiation() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let box_base = interner.lazy(DefId(42));
    let box_t = interner.application(box_base, vec![t_type]);

    let ctor = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: box_t,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    };

    let result = infer_generic_function(&interner, &mut subtype, &ctor, &[TypeId::NUMBER]);
    let expected = interner.application(box_base, vec![TypeId::NUMBER]);
    assert_eq!(result, expected);
}

#[test]
fn test_infer_generic_application_param() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let promise_base = interner.lazy(DefId(77));
    let promise_t = interner.application(promise_base, vec![t_type]);

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: promise_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let arg = interner.application(promise_base, vec![TypeId::NUMBER]);
    let result = infer_generic_function(&interner, &mut subtype, &func, &[arg]);
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_generic_call_uses_contextual_return_inference_for_application() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let ok_base = interner.lazy(DefId(500));
    let ok_t = interner.application(ok_base, vec![t_type]);
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
    let ok_tuple = interner.application(ok_base, vec![tuple]);

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("value")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: ok_t,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let arg = interner.array(interner.union(vec![
        interner.literal_string("hello"),
        interner.literal_number(12.0),
    ]));

    evaluator.set_contextual_type(Some(ok_tuple));
    let result = evaluator.resolve_call(func, &[arg]);

    match result {
        CallResult::Success(ret) => {
            let Some(TypeData::Application(app_id)) = interner.lookup(ret) else {
                panic!(
                    "Expected application return type, got {:?}",
                    interner.lookup(ret)
                );
            };
            let app = interner.type_application(app_id);
            assert_eq!(app.base, ok_base);
            assert_eq!(app.args.len(), 1);
            let Some(TypeData::Array(elem)) = interner.lookup(app.args[0]) else {
                panic!(
                    "Expected array type argument, got {:?}",
                    interner.lookup(app.args[0])
                );
            };
            let Some(TypeData::Union(list_id)) = interner.lookup(elem) else {
                panic!(
                    "Expected union element type, got {:?}",
                    interner.lookup(elem)
                );
            };
            let members = interner.type_list(list_id);
            assert_eq!(members.len(), 2);
        }
        other => panic!("Expected contextual return inference success, got {other:?}"),
    }
}

#[test]
fn test_generic_callback_instantiation_preserves_parameter_conflicts() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let callback_t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let callback_t_type = interner.intern(TypeData::TypeParameter(callback_t_param));
    let generic_callback = interner.function(FunctionShape {
        type_params: vec![callback_t_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: callback_t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: callback_t_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: callback_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer_t_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let outer_t_type = interner.intern(TypeData::TypeParameter(outer_t_param));
    let expected_callback = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("y")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: outer_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let higher_order = interner.function(FunctionShape {
        type_params: vec![outer_t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: expected_callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: outer_t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(higher_order, &[generic_callback]);
    // tsc rejects this: a generic callback <T>(x: T, y: T) => T cannot be
    // contextually instantiated against (x: number, y: string) => U because the
    // single naked type parameter T receives disjoint candidates (number and
    // string) from the two parameter positions. See conformance test
    // contextualSignatureInstantiation.ts which expects TS2345 here.
    assert!(
        matches!(result, CallResult::ArgumentTypeMismatch { .. }),
        "Expected generic callback to be rejected (conflicting T candidates), got {result:?}"
    );
}
