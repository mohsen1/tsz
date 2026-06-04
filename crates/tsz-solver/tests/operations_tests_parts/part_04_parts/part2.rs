#[test]
fn test_rest_param_nullable_prefix_reports_later_incompatible_argument() {
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
    let array_t = interner.array(t_type);

    let func = interner.function(FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: array_t,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let bad_string = interner.literal_string("x");
    let expected = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED, TypeId::NULL]);

    let result = evaluator.resolve_call(
        func,
        &[
            TypeId::BOOLEAN_FALSE,
            TypeId::UNDEFINED,
            TypeId::NULL,
            bad_string,
        ],
    );

    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected: actual_expected,
            actual,
            ..
        } => {
            assert_eq!(index, 3, "expected the later incompatible rest arg to fail");
            assert_eq!(
                actual_expected, expected,
                "expected nullable boolean inference for the rest element type"
            );
            assert_eq!(actual, bad_string);
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_array_constructor_rest_mismatch_keeps_nullable_fallback_array() {
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
    let array_t = interner.array(t_type);

    let array_ctor = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: Vec::new(),
        construct_signatures: vec![
            CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("arrayLength")),
                    type_id: TypeId::NUMBER,
                    optional: true,
                    rest: false,
                }],
                this_type: None,
                return_type: interner.array(TypeId::ANY),
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![t_param],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("arrayLength")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: array_t,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![t_param],
                params: vec![ParamInfo {
                    name: Some(interner.intern_string("items")),
                    type_id: array_t,
                    optional: false,
                    rest: true,
                }],
                this_type: None,
                return_type: array_t,
                type_predicate: None,
                is_method: false,
            },
        ],
        properties: Vec::new(),
        ..Default::default()
    });

    let bad_string = interner.literal_string("x");
    let expected_elem = interner.union(vec![TypeId::BOOLEAN, TypeId::UNDEFINED, TypeId::NULL]);
    let expected_array = interner.array(expected_elem);

    let result = evaluator.resolve_new(
        array_ctor,
        &[
            TypeId::BOOLEAN_FALSE,
            TypeId::UNDEFINED,
            TypeId::NULL,
            bad_string,
        ],
    );

    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            fallback_return,
        } => {
            assert_eq!(
                index, 3,
                "expected the rest overload to fail on the string item"
            );
            assert_eq!(expected, expected_elem);
            assert_eq!(actual, bad_string);
            assert_eq!(
                fallback_return, expected_array,
                "expected recovery to keep the nullable element type"
            );
        }
        _ => panic!("Expected ArgumentTypeMismatch, got {result:?}"),
    }
}

/// Test rest parameter with leading fixed parameters
/// function foo<T, U>(first: T, ...rest: U[]): [T, U]
#[test]
fn test_rest_param_with_leading_fixed() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let u_type = interner.intern(TypeData::TypeParameter(u_param));
    let array_u = interner.array(u_type);

    let return_tuple = interner.tuple(vec![
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = FunctionShape {
        type_params: vec![t_param, u_param],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("first")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("rest")),
                type_id: array_u,
                optional: false,
                rest: true,
            },
        ],
        this_type: None,
        return_type: return_tuple,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    // first: string, rest: number, number -> [string, number]
    let result = infer_generic_function(
        &interner,
        &mut subtype,
        &func,
        &[TypeId::STRING, TypeId::NUMBER, TypeId::NUMBER],
    );
    let expected = interner.tuple(vec![
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
    assert_eq!(result, expected);
}
