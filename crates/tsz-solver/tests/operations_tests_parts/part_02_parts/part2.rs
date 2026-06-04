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
