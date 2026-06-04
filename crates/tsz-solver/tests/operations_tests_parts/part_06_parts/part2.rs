#[test]
fn test_generic_call_infers_type_param_from_this_parameter() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let t_name = interner.intern_string("T");
    let t_info = TypeParamInfo {
        is_const: false,
        name: t_name,
        constraint: None,
        default: None,
    };
    let t_type = interner.type_param(t_info);

    let arg_type = interner.keyof(t_type);
    let foo = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(arg_type)],
        this_type: Some(t_type),
        return_type: TypeId::VOID,
        type_params: vec![t_info],
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let receiver = interner.object(vec![
        PropertyInfo::new(interner.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("b"), TypeId::STRING),
    ]);
    evaluator.set_actual_this_type(Some(receiver));

    let result = evaluator.resolve_call(foo, &[interner.literal_string("a")]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected generic `this` to infer T from receiver, got {result:?}"
    );
}

/// When a conditional constraint evaluates to a concrete type (not never),
/// inference should succeed normally.
///
/// Pattern: `<T extends null extends T ? any : never>(value: T): void`
/// Called with `string | null` → constraint is `null extends (string | null) ? any : never` → `any`
/// → `string | null` is assignable to `any` → OK
#[test]
fn test_generic_call_conditional_constraint_accepts_nullable() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut checker);

    let tp_name = interner.intern_string("T");
    let tp = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: None,
        default: None,
    };
    let tp_id = interner.type_param(tp);

    // Conditional: null extends T ? any : never
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::NULL,
        extends_type: tp_id,
        true_type: TypeId::ANY,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let tp_with_constraint = TypeParamInfo {
        is_const: false,
        name: tp_name,
        constraint: Some(cond),
        default: None,
    };
    let tp_id_constrained = interner.type_param(tp_with_constraint);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo::unnamed(tp_id_constrained)],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: vec![tp_with_constraint],
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call with `string | null` — should succeed because null <: (string | null) → any
    let nullable = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    let result = evaluator.resolve_call(func, &[nullable]);
    assert!(
        matches!(result, CallResult::Success(_)),
        "Expected success for nullable argument, got {result:?}"
    );
}

/// Test that `extract_iterator_result_value_types` properly partitions
/// `IteratorResult` into yield (done:false) and return (done:true) types.
#[test]
fn test_extract_iterator_result_yield_vs_return() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();
    let done_atom = interner.intern_string("done");
    let value_atom = interner.intern_string("value");

    // Build: { done?: false, value: string } | { done: true, value: undefined }
    // This is what IteratorResult<string, undefined> expands to.
    let yield_branch = interner.object(vec![
        PropertyInfo::opt(done_atom, TypeId::BOOLEAN_FALSE), // done?: false
        PropertyInfo::new(value_atom, TypeId::STRING),       // value: string
    ]);

    let return_branch = interner.object(vec![
        PropertyInfo::new(done_atom, TypeId::BOOLEAN_TRUE), // done: true
        PropertyInfo::new(value_atom, TypeId::UNDEFINED),   // value: undefined
    ]);

    let iterator_result = interner.union(vec![yield_branch, return_branch]);

    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, iterator_result);

    assert_eq!(
        yield_type,
        TypeId::STRING,
        "yield type should be string (from done:false branch)"
    );
    assert_eq!(
        return_type,
        TypeId::UNDEFINED,
        "return type should be undefined (from done:true branch)"
    );
}

/// Test that `extract_iterator_result_value_types` extracts args from Application types.
/// For `IteratorResult<T, TReturn>`, args[0] = T (yield), args[1] = `TReturn` (return).
#[test]
fn test_extract_iterator_result_application_extracts_args() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();

    // Simulate IteratorResult<string, undefined> as an Application type
    // base=some_type, args=[string, undefined]
    let app = interner.application(TypeId::STRING, vec![TypeId::STRING, TypeId::UNDEFINED]);
    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, app);

    assert_eq!(
        yield_type,
        TypeId::STRING,
        "should extract args[0] as yield type from Application"
    );
    assert_eq!(
        return_type,
        TypeId::UNDEFINED,
        "should extract args[1] as return type from Application"
    );
}

/// Test that a single-object `IteratorResult` (no union) extracts value as yield type.
#[test]
fn test_extract_iterator_result_single_object() {
    use crate::operations::extract_iterator_result_value_types;

    let interner = TypeInterner::new();
    let value_atom = interner.intern_string("value");

    // Build: { value: number } — a simple object with a value property
    let obj = interner.object(vec![PropertyInfo::new(value_atom, TypeId::NUMBER)]);

    let (yield_type, return_type) = extract_iterator_result_value_types(&interner, obj);

    assert_eq!(
        yield_type,
        TypeId::NUMBER,
        "single object yield should be the value type"
    );
    assert_eq!(
        return_type,
        TypeId::ANY,
        "single object return should be ANY"
    );
}

#[test]
fn test_call_optional_param_accepts_union_with_undefined() {
    // Regression test: calling `f(message?: string)` with arg `string | undefined`
    // should succeed — the optional param implicitly accepts `undefined`.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    // function(message?: string): never
    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("message")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NEVER,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Arg: string | undefined
    let string_or_undef = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);

    let result = evaluator.resolve_call(func, &[string_or_undef]);
    match result {
        CallResult::Success(ret) => assert_eq!(ret, TypeId::NEVER),
        other => {
            panic!("Expected Success for optional param with string | undefined arg, got {other:?}")
        }
    }
}

#[test]
fn test_call_optional_param_rejects_wrong_type_with_undefined() {
    // Calling `f(x?: string)` with `number | undefined` should still fail —
    // only `undefined` is stripped, leaving `number` which is not assignable to `string`.
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);
    let mut evaluator = CallEvaluator::new(&interner, &mut subtype);

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Arg: number | undefined
    let num_or_undef = interner.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]);

    let result = evaluator.resolve_call(func, &[num_or_undef]);
    match result {
        CallResult::ArgumentTypeMismatch { .. } => {} // expected
        other => {
            panic!("Expected ArgumentTypeMismatch for number|undefined -> string?, got {other:?}")
        }
    }
}
