#[test]
fn test_nongeneric_construct_sig_nested_callback_not_assignable_to_generic_target() {
    let interner = TypeInterner::new();

    let base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("base"),
        TypeId::NUMBER,
    )]);
    let derived = interner.object(vec![
        PropertyInfo::new(interner.intern_string("base"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("derived"), TypeId::NUMBER),
    ]);
    let derived2 = interner.object(vec![
        PropertyInfo::new(interner.intern_string("base"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("derived2"), TypeId::NUMBER),
    ]);

    let source_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
            type_id: base,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: derived,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let source_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("r")),
            type_id: base,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: derived2,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let source = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: source_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: source_return,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        ..Default::default()
    });

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: Some(base),
        default: None,
        is_const: false,
    };
    let u_param = TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(derived),
        default: None,
        is_const: false,
    };
    let v_param = TypeParamInfo {
        name: interner.intern_string("V"),
        constraint: Some(derived2),
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let u_type = interner.type_param(u_param);
    let v_type = interner.type_param(v_param);
    let target_param = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("arg")),
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
    let target_return = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("r")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: v_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let target = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![t_param, u_param, v_param],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: target_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: target_return,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        ..Default::default()
    });

    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_function_types = false;
    checker.erase_generics = true;
    assert!(checker.check_subtype(source, target).is_false());
}

/// Regression test for genericFunctionCallSignatureReturnTypeMismatch.ts (TS2322)
///
/// `{ <S>(): S[] }` should NOT be a subtype of `{ <T>(x: T): T }` because:
/// - After alpha-renaming T → S, target becomes `(x: S) => S`
/// - Source is `() => S[]`
/// - Return type: S[] is NOT assignable to S (concrete type not assignable to type param)
#[test]
fn test_generic_callable_return_type_mismatch_not_assignable() {
    let interner = TypeInterner::new();

    let s_param = TypeParamInfo {
        name: interner.intern_string("S"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_type = interner.type_param(s_param);
    let s_array = interner.array(s_type);
    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![s_param],
            params: vec![],
            this_type: None,
            return_type: s_array,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.type_param(t_param);
    let target = interner.callable(CallableShape {
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
        construct_signatures: vec![],
        properties: vec![],
        ..Default::default()
    });

    let mut checker = SubtypeChecker::new(&interner);
    checker.strict_function_types = true;
    assert!(
        !checker.is_subtype_of(source, target),
        "generic callable with incompatible return type should not be a subtype"
    );
}
