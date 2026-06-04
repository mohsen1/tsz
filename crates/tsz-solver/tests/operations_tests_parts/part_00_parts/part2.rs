/// Calling a variadic tuple rest param function with too few args should produce
/// `ArgumentTypeMismatch` (TS2345), not `ArgumentCountMismatch` (TS2555).
/// E.g. `f1(...args: [...T[], Required])` called as `f1()` → TS2345.
#[test]
fn test_call_variadic_tuple_rest_empty_args_produces_type_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // Build tuple type: [...((arg: number) => void)[], (arg: string) => void]
    let num_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let str_fn = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let rest_array = interner.array(num_fn);
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: str_fn,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with 0 args — should get ArgumentTypeMismatch (TS2345), not ArgumentCountMismatch
    let result = evaluator.resolve_call(func, &[]);
    match result {
        CallResult::ArgumentTypeMismatch {
            expected, actual, ..
        } => {
            // Expected: the variadic tuple type
            assert_eq!(expected, tuple_type);
            // Actual: an empty tuple []
            assert!(
                matches!(interner.lookup(actual), Some(TypeData::Tuple(elems)) if interner.tuple_list(elems).is_empty()),
                "Expected empty tuple for actual, got {:?}",
                interner.lookup(actual)
            );
        }
        _ => panic!(
            "Expected ArgumentTypeMismatch for empty args to variadic tuple rest, got {result:?}"
        ),
    }

    // Call with 1 arg (the required trailing element) — should succeed
    let result = evaluator.resolve_call(func, &[str_fn]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::VOID),
        _ => panic!("Expected success with 1 arg to variadic tuple rest, got {result:?}"),
    }
}

#[test]
fn test_call_variadic_tuple_rest_with_trailing_element_uses_aggregate_mismatch() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let rest_array = interner.array(TypeId::STRING);
    let tuple_type = interner.tuple(vec![
        TupleElement {
            type_id: rest_array,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("args")),
            type_id: tuple_type,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_call(func, &[TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    match result {
        CallResult::ArgumentTypeMismatch {
            index,
            expected,
            actual,
            ..
        } => {
            assert_eq!(index, 0);
            assert_eq!(expected, tuple_type);
            let Some(TypeData::Tuple(actual_elements)) = interner.lookup(actual) else {
                panic!(
                    "expected aggregate tuple actual, got {:?}",
                    interner.lookup(actual)
                );
            };
            let actual_elements = interner.tuple_list(actual_elements);
            assert_eq!(actual_elements.len(), 3);
            assert_eq!(actual_elements[1].type_id, TypeId::NUMBER);
        }
        _ => panic!("Expected aggregate ArgumentTypeMismatch, got {result:?}"),
    }
}

#[test]
fn test_property_access_on_never_returns_never() {
    // never is the bottom type — all property accesses are vacuously valid
    // and return never (the code is unreachable). tsc does not emit TS2339 on never.
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let result = evaluator.resolve_property_access(TypeId::NEVER, "anything");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Property access on never should succeed with never, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(TypeId::NEVER, "nonexistent");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NEVER),
        _ => panic!("Any property on never should return never, got {result:?}"),
    }
}

#[test]
fn test_property_access_object() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    // { x: number, y: string }
    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    // Access existing property
    let result = evaluator.resolve_property_access(obj, "x");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    // Access non-existent property
    let result = evaluator.resolve_property_access(obj, "z");
    match result {
        PropertyAccessResult::PropertyNotFound { .. } => {}
        _ => panic!("Expected PropertyNotFound, got {result:?}"),
    }
}

#[test]
fn test_property_access_function_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let func = interner.function(FunctionShape {
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = evaluator.resolve_property_access(func, "call");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected call to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            let rest_array = interner.array(TypeId::ANY);
            assert_eq!(shape.return_type, TypeId::ANY);
            assert_eq!(shape.params.len(), 1);
            assert!(shape.params[0].rest);
            assert_eq!(shape.params[0].type_id, rest_array);
        }
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "length");
    match result {
        PropertyAccessResult::Success { type_id: t, .. } => assert_eq!(t, TypeId::NUMBER),
        _ => panic!("Expected success, got {result:?}"),
    }

    let result = evaluator.resolve_property_access(func, "toString");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected toString to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::STRING);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_callable_members() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let call_sig = CallSignature {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method: false,
    };
    let callable = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![call_sig],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let result = evaluator.resolve_property_access(callable, "bind");
    match result {
        PropertyAccessResult::Success { type_id, .. } => {
            let Some(TypeData::Function(shape_id)) = interner.lookup(type_id) else {
                panic!("Expected bind to resolve to function type");
            };
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::ANY);
        }
        _ => panic!("Expected success, got {result:?}"),
    }
}

#[test]
fn test_property_access_optional_property() {
    let interner = TypeInterner::new();
    let evaluator = PropertyAccessEvaluator::new(&interner);

    let obj = interner.object(vec![PropertyInfo::opt(
        interner.intern_string("x"),
        TypeId::NUMBER,
    )]);

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
