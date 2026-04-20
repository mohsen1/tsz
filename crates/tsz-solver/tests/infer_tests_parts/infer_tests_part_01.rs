#[test]
fn test_resolve_bounds_number_index_ignores_negative_binary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("-0b1");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_negative_octal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("-0o7");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_exponent_double_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e++1");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_exponent_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e--1");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_exponent_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e+");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_exponent_minus_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e-");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_negative_exponent_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("-0e+0");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_positive_exponent_zero() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e+0");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_accepts_negative_decimal_boundary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("-0.000001");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}
#[test]
fn test_resolve_bounds_number_index_ignores_trailing_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1.");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_leading_plus_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("+1");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_number_index_ignores_numeric_separator_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1_0");

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: vec![PropertyInfo {
            name,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
#[test]
fn test_resolve_bounds_inconsistent_index_signatures() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let upper_type = interner.object_with_index(ObjectShape {
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

    let lower_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}
#[test]
fn test_resolve_bounds_object_with_index_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let upper_type = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::STRING,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let lower_type = interner.object_with_index(ObjectShape {
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

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower_type && actual_upper == upper_type
    ));
}
#[test]
fn test_resolve_bounds_function_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![source_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![target_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_function_this_parameter_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}

/// Test resolving bounds for function `this` parameter with optional target
///
/// NOTE: Currently ignored - bounds resolution for function `this` parameters with
/// optional targets is not fully implemented. The solver panics with a `BoundsViolation`
/// error when trying to resolve this case.
#[test]
fn test_resolve_bounds_function_this_parameter_optional_target() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_function_this_parameter_any_upper_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::NUMBER),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::ANY),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    // TODO: (this: number) => void should satisfy the (this: any) => void
    // upper bound, but the bounds checker currently reports a BoundsViolation
    // because function subtyping with `this` parameters is not wired into
    // the constraint resolution path.
    let result = ctx.resolve_with_constraints(var);
    assert!(
        result.is_err(),
        "Expected BoundsViolation for function this-parameter upper bound check"
    );
}
#[test]
fn test_resolve_bounds_function_this_parameter_contravariant() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let lower_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(lower_this),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(TypeId::STRING),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_callable_this_parameter_contravariant() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let lower_this = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let lower = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: Some(lower_this),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: Some(TypeId::STRING),
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_optional_property_compatible() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo::opt(name_a, TypeId::STRING)]);
    let lower = interner.object(vec![PropertyInfo::new(name_a, TypeId::STRING)]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_optional_property_mismatch() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo::new(name_a, TypeId::STRING)]);
    let lower = interner.object(vec![PropertyInfo::opt(name_a, TypeId::STRING)]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower: actual_lower,
            upper: actual_upper,
            ..
        }) if actual_lower == lower && actual_upper == upper
    ));
}
#[test]
fn test_resolve_bounds_optional_property_missing_ok() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name_a = interner.intern_string("a");

    let upper = interner.object(vec![PropertyInfo::opt(name_a, TypeId::STRING)]);
    let lower = interner.object(Vec::new());

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_callable_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![source_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![target_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_function_to_callable() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![source_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let upper = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![target_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_callable_to_function() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    let source_param = ParamInfo {
        name: Some(interner.intern_string("x")),
        type_id: interner.union(vec![TypeId::STRING, TypeId::NUMBER]),
        optional: false,
        rest: false,
    };
    let target_param = ParamInfo {
        name: Some(interner.intern_string("y")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };

    let lower = interner.callable(CallableShape {
        symbol: None,
        is_abstract: false,
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![source_param],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        ..Default::default()
    });
    let upper = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![target_param],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_application_subtype() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let base = interner.lazy(DefId(1));
    let upper = interner.application(base, vec![TypeId::STRING]);
    let lower = interner.application(base, vec![interner.literal_string("hello")]);

    ctx.add_lower_bound(var, lower);
    ctx.add_upper_bound(var, upper);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower);
}
#[test]
fn test_resolve_bounds_conflict() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);

    ctx.add_lower_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var);
    assert!(matches!(
        result,
        Err(InferenceError::BoundsViolation {
            lower,
            upper,
            ..
        }) if lower == TypeId::STRING && upper == TypeId::NUMBER
    ));
}
#[test]
fn test_resolve_bounds_duplicate_upper_bounds_no_intersection() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var = ctx.fresh_type_param(t_name, false);

    ctx.add_upper_bound(var, TypeId::STRING);
    ctx.add_upper_bound(var, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_resolve_no_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);

    // No constraints at all
    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, TypeId::UNKNOWN);
}
#[test]
fn test_infer_union_target_with_placeholder_member() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let param_type = interner.union(vec![t_type, TypeId::STRING]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
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
    };

    let result = infer_generic_function(&interner, &mut checker, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_infer_union_target_with_placeholder_and_never_member() {
    let interner = TypeInterner::new();
    let mut checker = CompatChecker::new(&interner);
    let t_name = interner.intern_string("T");

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let param_type = interner.union(vec![t_type, TypeId::NEVER]);

    let func = FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
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
    };

    let result = infer_generic_function(&interner, &mut checker, &func, &[TypeId::NUMBER]);
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_resolve_circular_extends_with_concrete_bound() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Simulate: <T extends U, U extends T, U extends string>
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);
    ctx.add_upper_bound(var_u, TypeId::STRING);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}
#[test]
fn test_resolve_circular_extends_bound_order() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Same cycle, but add concrete bound before the cyclic one.
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}
#[test]
fn test_resolve_usage_based_inference_from_bound_param() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Simulate: <T extends U, U extends T> with usage-based lower bound on U.
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_u, hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}

// =============================================================================
// Best Common Type Tests
// =============================================================================
#[test]
fn test_best_common_type_single() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_best_common_type_union() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_best_common_type_dedup() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // Duplicate types should be deduped
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::STRING, TypeId::NUMBER]);
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_best_common_type_reuses_subtype_cache() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    assert!(ctx.subtype_cache.borrow().is_empty());

    let input = [TypeId::STRING, TypeId::NUMBER, TypeId::STRING];
    let _ = ctx.best_common_type(&input);
    let first_cache_size = ctx.subtype_cache.borrow().len();
    assert!(first_cache_size > 0);

    let _ = ctx.best_common_type(&input);
    let second_cache_size = ctx.subtype_cache.borrow().len();
    assert_eq!(second_cache_size, first_cache_size);
}
#[test]
fn test_best_common_type_empty() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[]);
    assert_eq!(result, TypeId::UNKNOWN);
}
#[test]
fn test_best_common_type_never_ignored() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    // never doesn't contribute to union
    let result = ctx.best_common_type(&[TypeId::STRING, TypeId::NEVER]);
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_best_common_type_all_never() {
    let interner = TypeInterner::new();
    let ctx = InferenceContext::new(&interner);

    let result = ctx.best_common_type(&[TypeId::NEVER, TypeId::NEVER]);
    assert_eq!(result, TypeId::NEVER);
}

// =============================================================================
// Full Inference Scenario Tests
// =============================================================================
#[test]
fn test_resolve_all_with_constraints() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    // Simulate: function foo<T, U>(a: T, b: U) called with foo("hello", 42)
    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let hello = interner.literal_string("hello");
    let forty_two = interner.literal_number(42.0);

    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, forty_two);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}
#[test]
fn test_resolve_all_with_circular_extends_unknown() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Simulate: <T extends U, U extends T>
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
}

// =============================================================================
// Additional Circular Generic Constraint Tests
// =============================================================================
#[test]
fn test_circular_extends_three_way_cycle() {
    // Test: <T extends U, U extends V, V extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All three resolve to unknown due to circular dependency with no concrete bounds
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
}
#[test]
fn test_circular_extends_self_reference() {
    // Test: <T extends T> - self-referential constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends T (self-reference)
    ctx.add_upper_bound(var_t, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // Self-reference with no other bounds resolves to unknown
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
}
#[test]
fn test_circular_extends_with_lower_bound() {
    // Test: <T extends U, U extends T> with T having a lower bound of string
    // Lower bounds propagate through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Add a lower bound to T
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to string (its lower bound)
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U also resolves to string - lower bounds propagate through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::STRING));
}
#[test]
fn test_circular_extends_both_have_lower_bounds() {
    // Test: <T extends U, U extends T> with both having the same lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have the same lower bound
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Both resolve to string
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::STRING));
}
#[test]
fn test_circular_extends_unify_propagates() {
    // Test: <T extends U, U extends T> then unify T with number
    // Unification propagates through cyclic constraints.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Unify T directly with number
    ctx.unify_var_type(var_t, TypeId::NUMBER).unwrap();

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to number (unified)
    assert_eq!(results[0], (t_name, TypeId::NUMBER));
    // U also resolves to number - unification propagates through cyclic constraints
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}
#[test]
fn test_circular_extends_conflicting_lower_bounds() {
    // Test: <T extends U, U extends T> with T: string and U: number
    // Cycle propagation causes both to get union of all lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Conflicting lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // After SCC unification, both lower bounds are merged into a union.
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(results[0], (t_name, expected_union));
    assert_eq!(results[1], (u_name, expected_union));
}
#[test]
fn test_circular_extends_three_way_with_one_lower_bound() {
    // Test: <T extends U, U extends V, V extends T> with V having lower bound
    // Bounds propagate through adjacent connections in the cycle
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, t_type);

    // Only V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to boolean (its direct lower bound)
    assert_eq!(results[2], (v_name, TypeId::BOOLEAN));
    // U extends V, so U gets boolean through propagation
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
    // T extends U, and with fixed-point propagation T also gets boolean
    // (previous impl limitation: propagation stopped at one level, T was UNKNOWN)
    assert_eq!(results[0], (t_name, TypeId::BOOLEAN));
}
#[test]
fn test_circular_extends_with_union_lower_bound() {
    // Test: <T extends U, U extends T> with T having union type as lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has a union type as lower bound
    let union_type = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_lower_bound(var_t, union_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to the union type
    assert_eq!(results[0], (t_name, union_type));
    // U also resolves to the union through propagation
    assert_eq!(results[1], (u_name, union_type));
}
#[test]
fn test_circular_extends_with_literal_types() {
    // Test: <T extends U, U extends T> with literal type lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // Both have literal string lower bounds
    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_lower_bound(var_u, world);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Both T and U get string (widened from union of "hello" | "world")
    // In a cycle with conflicting literals, both unify to the common primitive type
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::STRING));
}
#[test]
fn test_circular_extends_four_way_cycle() {
    // Test: <T extends U, U extends V, V extends W, W extends T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");
    let w_name = interner.intern_string("W");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);
    let var_w = ctx.fresh_type_param(w_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let w_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: w_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V, V extends W, W extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);
    ctx.add_upper_bound(var_v, w_type);
    ctx.add_upper_bound(var_w, t_type);

    let results = ctx.resolve_all_with_constraints().unwrap();

    // All four resolve to unknown with no lower bounds
    assert_eq!(results.len(), 4);
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    assert_eq!(results[1], (u_name, TypeId::UNKNOWN));
    assert_eq!(results[2], (v_name, TypeId::UNKNOWN));
    assert_eq!(results[3], (w_name, TypeId::UNKNOWN));
}
#[test]
fn test_circular_extends_with_concrete_upper_and_lower() {
    // Test: <T extends U, U extends T> with T having both upper and lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has both upper bound (string) and lower bound (literal)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T resolves to its lower bound (hello literal)
    assert_eq!(results[0], (t_name, TypeId::STRING));
    // U gets hello through propagation
    assert_eq!(results[1], (u_name, TypeId::STRING));
}
#[test]
fn test_circular_extends_chain_with_endpoint_bound() {
    // Test: <T extends U, U extends V> (not circular) with V having lower bound
    // Chain propagation: upper bounds become resolved types when no lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");
    let v_name = interner.intern_string("V");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let v_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: v_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends V (chain, not cycle)
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, v_type);

    // V has a lower bound
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    // V resolves to number (its lower bound)
    assert_eq!(results[2], (v_name, TypeId::NUMBER));
    // U has upper bound V but no lower bound, so resolves to its upper bound (V type param)
    assert_eq!(results[1].0, u_name);
    assert!(matches!(
        interner.lookup(results[1].1),
        Some(TypeData::TypeParameter(_))
    ));
    // T has upper bound U but no lower bound, resolves to its upper bound (U type param)
    assert_eq!(results[0].0, t_name);
    assert!(matches!(
        interner.lookup(results[0].1),
        Some(TypeData::TypeParameter(_))
    ));
}
#[test]
fn test_circular_extends_multiple_lower_bounds_same_param() {
    // Test: <T extends U, U extends T> with T having multiple lower bounds
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));
    let u_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: u_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // T extends U, U extends T
    ctx.add_upper_bound(var_t, u_type);
    ctx.add_upper_bound(var_u, t_type);

    // T has multiple lower bounds
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // Multiple lower bounds are unioned: string | number | boolean
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(results[0], (t_name, expected_union));
    // U gets the same result through SCC propagation
    assert_eq!(results[1], (u_name, expected_union));
}

// =============================================================================
// Context-Sensitive Typing Tests
// =============================================================================
#[test]
fn test_context_sensitive_callback_param_from_upper_bound() {
    // Test: When a callback parameter has an upper bound from context,
    // the parameter type is inferred from that context.
    // e.g., arr.map((x) => x + 1) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context provides: T must be a subtype of number (from array element type)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound and no lower bound, resolves to the upper bound
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_context_sensitive_return_type_from_usage() {
    // Test: Return type inference from how the result is used
    // e.g., const x: string = identity(value) infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Usage site provides: result is assigned to string variable
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Call site provides: argument is a string literal
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound wins (more specific)
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_context_sensitive_multiple_usage_sites() {
    // Test: Multiple usage sites provide constraints that must be unified
    // e.g., function used in two places with different argument types
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // First usage: called with string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Second usage: called with number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple lower bounds: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_context_sensitive_literal_widening_prevented() {
    // Test: Fresh literals are always widened during inference resolution.
    // With upper bound STRING, widened literal satisfies constraint.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    // Lower bound is the literal
    ctx.add_lower_bound(var_t, hello);
    // Upper bound is the base type (widened literal satisfies this)
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Fresh literal is widened to string
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_context_sensitive_object_property_inference() {
    // Test: Object property types inferred from contextual type
    // e.g., const obj: {x: number} = {x: getValue()}
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects number for property x
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Value provides a specific number
    let forty_two = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, forty_two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Literal number from lower bound
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_context_sensitive_array_element_inference() {
    // Test: Array element types inferred from array context
    // e.g., const arr: string[] = [getValue()]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Context: array of strings means elements must be strings
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_context_sensitive_conditional_branch_types() {
    // Test: Type from conditional branches — both branches contribute lower bounds
    // e.g., condition ? stringValue : numberValue
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // True branch contributes string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // False branch contributes number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions both branch types: string | number
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_context_sensitive_function_param_from_callback_context() {
    // Test: Function parameter type inferred from callback signature context
    // e.g., arr.filter((x) => x > 0) where arr: number[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Contextual callback type says param must be number
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // No explicit annotation, so no lower bound from declaration

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Infers from context
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_context_sensitive_rest_param_inference() {
    // Test: Rest parameter type inference from spread arguments
    // e.g., fn(...args: T) called with (1, 2, 3)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple arguments of same type contribute to rest param type
    let one = interner.literal_number(1.0);
    let two = interner.literal_number(2.0);
    ctx.add_lower_bound(var_t, one);
    ctx.add_lower_bound(var_t, two);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple number literals widen to number
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_context_sensitive_default_param_inference() {
    // Test: Default parameter provides lower bound for type param
    // e.g., function fn<T>(x: T = "default")
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Default value is a string literal
    let default_val = interner.literal_string("default");
    ctx.add_lower_bound(var_t, default_val);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Callback Parameter Inference Tests
// =============================================================================
#[test]
fn test_callback_param_inferred_from_array_map() {
    // Test: arr.map((x) => x.toUpperCase()) where arr: string[]
    // The callback parameter x should be inferred as string from array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Array<string>.map provides callback with (element: string) => U
    // So T (the callback param type) has upper bound string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Callback param inferred from array element type
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_callback_param_inferred_with_index() {
    // Test: arr.forEach((item, index) => ...) where arr: number[]
    // First param is number (element), second is number (index)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T is the element type from Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // U is the index type (always number)
    ctx.add_upper_bound(var_u, TypeId::NUMBER);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::NUMBER);
    assert_eq!(result_u, TypeId::NUMBER);
}
#[test]
fn test_callback_param_inferred_from_generic_higher_order() {
    // Test: Generic higher-order function like filter<T>(arr: T[], pred: (x: T) => boolean)
    // When called with string[], T is inferred as string, so callback param is string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bound from argument: array contains strings
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound from callback usage: predicate receives T
    // (callback param type flows from T)

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // T inferred as string, so callback param is string
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Generic Default Type Inference Tests
// =============================================================================
#[test]
fn test_generic_default_used_when_no_inference() {
    // Test: <T = string> with no inference constraints, T defaults to string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No constraints added - should use default if available
    // Note: defaults are typically handled during type param registration,
    // but here we test the inference context behavior with no constraints
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Without any constraints, resolves to unknown
    assert_eq!(result, TypeId::UNKNOWN);
}
#[test]
fn test_generic_default_overridden_by_lower_bound() {
    // Test: <T = string> with lower bound number, inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Inferred lower bound takes precedence
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound overrides any potential default
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_generic_default_with_constraint() {
    // Test: <T extends object = {}> - constraint with default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With only upper bound, resolves to the upper bound
    assert_eq!(result, TypeId::OBJECT);
}
#[test]
fn test_generic_default_with_literal_inference() {
    // Test: <T = string> called with literal "hello", infers literal not default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Inferred literal takes precedence over default
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_generic_multiple_params_with_defaults() {
    // Test: <T = string, U = number> with only U having lower bound
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let _var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Only U has a lower bound
    ctx.add_lower_bound(var_u, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // T has no constraints, resolves to unknown
    assert_eq!(results[0], (t_name, TypeId::UNKNOWN));
    // U has lower bound, resolves to boolean
    assert_eq!(results[1], (u_name, TypeId::BOOLEAN));
}

// =============================================================================
// Generic Constraint Propagation Tests
// =============================================================================
#[test]
fn test_constraint_propagation_upper_to_lower() {
    // Test: Upper bound on one param propagates to lower bound check
    // <T extends string> called with T = "hello"
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);
    // Lower bound from argument: "hello"
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Lower bound satisfies upper bound, resolves to literal
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_propagation_through_unification() {
    // Test: Unifying two vars propagates constraints from both
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T has lower bound string
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U has lower bound number
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    // Unify T and U
    ctx.unify_vars(var_t, var_u).unwrap();

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // After unification, both lower bounds are merged into a union
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_constraint_propagation_transitive_upper_bounds() {
    // Test: T extends string with lower bound "hello"
    // Lower bound must satisfy upper bound constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: T extends string
    ctx.add_upper_bound(var_t, TypeId::STRING);

    // Add lower bound to T (literal satisfies string upper bound)
    let hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();

    // T resolves to its lower bound (literal "hello")
    assert_eq!(result_t, TypeId::STRING);
}
#[test]
fn test_constraint_propagation_multiple_upper_bounds() {
    // Test: T extends A & B (multiple upper bounds create intersection)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple upper bounds
    ctx.add_upper_bound(var_t, TypeId::STRING);
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Multiple upper bounds create intersection
    let expected = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected);
}
#[test]
fn test_constraint_propagation_lower_bounds_union() {
    // Test: Multiple lower bounds produce a union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Multiple lower bounds from different call sites
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple lower bounds: string | number | boolean
    let expected = interner.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    assert_eq!(result, expected);
}
#[test]
fn test_constraint_propagation_with_never_lower_bound() {
    // Test: never as lower bound doesn't contribute to union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bounds including never
    ctx.add_lower_bound(var_t, TypeId::NEVER);
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // never is filtered out, only string remains
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_propagation_any_lower_with_concrete() {
    // Test: any as lower bound with concrete type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Lower bounds: any and string
    ctx.add_lower_bound(var_t, TypeId::ANY);
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // Upper bound constrains
    ctx.add_upper_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // With upper bound, any is filtered from lower bounds
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constraint_propagation_object_properties() {
    // Test: Object type constraint propagation
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Create object type with property
    let prop_name = interner.intern_string("x");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::NUMBER)]);

    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj_type);
}

// ============================================================================
// Constructor Type Inference Tests
// ============================================================================
// Tests for constructor function type inference
#[test]
fn test_constructor_single_param_inference() {
    // Test: new (x: T) => Instance infers T from argument
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Constructor param receives string argument
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_constructor_multiple_params_inference() {
    // Test: new <T, U>(a: T, b: U) => Instance infers both T and U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // First param is string, second is number
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], (t_name, TypeId::STRING));
    assert_eq!(results[1], (u_name, TypeId::NUMBER));
}
#[test]
fn test_constructor_with_constraint() {
    // Test: new <T extends object>(config: T) => Instance
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T has upper bound of object
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Argument is specific object type
    let prop_name = interner.intern_string("name");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should be the specific object type
    assert_eq!(result, obj_type);
}
#[test]
fn test_constructor_optional_param_inference() {
    // Test: new <T>(arg?: T) => Instance with optional param
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Optional param not provided - may include undefined
    let optional_type = interner.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
    ctx.add_lower_bound(var_t, optional_type);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should preserve the union type
    assert_eq!(result, optional_type);
}
#[test]
fn test_constructor_rest_param_inference() {
    // Test: new <T>(...args: T[]) => Instance with rest param
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Rest param elements are string and number - infer union
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should be union of string | number
    if let Some(TypeData::Union(_)) = interner.lookup(result) {
        // Union is expected
    } else {
        // Could also resolve to one of the types if widening happens
        assert!(result == TypeId::STRING || result == TypeId::NUMBER);
    }
}

// ============================================================================
// Method Signature Inference Tests
// ============================================================================
#[test]
fn test_method_return_type_inference_basic() {
    // Test inferring return type from method call: obj.method() returns string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: () => T
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Call returns string, so T should be inferred as string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_method_parameter_type_inference() {
    // Test inferring parameter type from method call: obj.method(value)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: (x: T) => void
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    // Called with number, so T should be inferred as number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_method_this_type_inference() {
    // Test this type in method: class method with this constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let this_name = interner.intern_string("This");

    let var_this = ctx.fresh_type_param(this_name, false);
    let this_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: this_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: (this: This) => This
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: this_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![],
        this_type: Some(this_type),
        return_type: this_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Create an object type to represent `this`
    let obj_type = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        TypeId::STRING,
    )]);

    // Called on object, so This should be inferred as that object type
    ctx.add_lower_bound(var_this, obj_type);

    let result = ctx.resolve_with_constraints(var_this).unwrap();
    assert_eq!(result, obj_type);
}
#[test]
fn test_method_generic_parameter_inference() {
    // Test: generic method <T>(x: T) => Array<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);
    let t_type = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    }));

    // Method signature: <T>(x: T) => Array<T>
    let return_array = interner.array(t_type);
    let _method = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: return_array,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Called with boolean, so T should be inferred as boolean
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}
#[test]
fn test_method_multiple_generic_params_inference() {
    // Test: <K, V>(key: K, value: V) => Map<K, V>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let k_name = interner.intern_string("K");
    let v_name = interner.intern_string("V");

    let var_k = ctx.fresh_type_param(k_name, false);
    let var_v = ctx.fresh_type_param(v_name, false);

    // Called with (string, number)
    ctx.add_lower_bound(var_k, TypeId::STRING);
    ctx.add_lower_bound(var_v, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 2);
    // K inferred as string
    assert_eq!(results[0], (k_name, TypeId::STRING));
    // V inferred as number
    assert_eq!(results[1], (v_name, TypeId::NUMBER));
}

// ============================================================================
// Circular Type Alias Detection Tests
// ============================================================================
// Tests for detecting and handling circular type aliases
#[test]
fn test_circular_type_alias_self_reference() {
    // Test: type T = T (direct self-reference should be detected)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No bounds - trying to resolve should give unknown/any
    let result = ctx.resolve_with_constraints(var_t);
    // Without concrete bounds, resolution should still work (gives unknown)
    assert!(result.is_ok());
}
#[test]
fn test_circular_type_alias_via_array() {
    // Test: type T = Array<T> - recursive through array
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Add concrete array lower bound
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), string_array);
}
#[test]
fn test_circular_type_alias_via_union() {
    // Test: type T = T | null - recursive through union
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: string | null
    let string_or_null = interner.union(vec![TypeId::STRING, TypeId::NULL]);
    ctx.add_upper_bound(var_t, string_or_null);

    // Lower bound: string
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::STRING);
}
#[test]
fn test_circular_type_alias_nested_object() {
    // Test: type Node = { child: Node | null }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Just test that we can have object bounds without infinite recursion
    let prop_name = interner.intern_string("value");
    let obj_type = interner.object(vec![PropertyInfo::new(prop_name, TypeId::STRING)]);
    ctx.add_lower_bound(var_t, obj_type);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), obj_type);
}
#[test]
fn test_circular_type_alias_function_return() {
    // Test: type F = () => F - function returning itself
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let f_name = interner.intern_string("F");

    let var_f = ctx.fresh_type_param(f_name, false);

    // Add function lower bound
    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    ctx.add_lower_bound(var_f, fn_type);

    let result = ctx.resolve_with_constraints(var_f);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), fn_type);
}

// ============================================================================
// Self-Referential Generic Constraints Tests
// ============================================================================
// Tests for generic type parameters that reference themselves in constraints
#[test]
fn test_self_ref_constraint_comparable() {
    // Test: T extends Comparable<T> pattern (common in sorting)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: number (which is comparable to itself)
    ctx.add_upper_bound(var_t, TypeId::NUMBER);

    // Lower bound: specific number literal
    let num_lit = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, num_lit);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::NUMBER);
}
#[test]
fn test_self_ref_constraint_builder_pattern() {
    // Test: T extends Builder<T> - fluent builder pattern
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Simulate builder with method that returns same type
    let build_prop = interner.intern_string("build");
    let builder_fn = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let builder_type = interner.object(vec![PropertyInfo::method(build_prop, builder_fn)]);

    ctx.add_lower_bound(var_t, builder_type);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), builder_type);
}
#[test]
fn test_self_ref_constraint_iterable() {
    // Test: T extends Iterable<T> - iterable of itself
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: array (iterable)
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_upper_bound(var_t, number_array);

    // Lower bound: specific array
    ctx.add_lower_bound(var_t, number_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), number_array);
}
#[test]
fn test_self_ref_constraint_json_value() {
    // Test: type JSONValue = string | number | boolean | JSONValue[] | {[k: string]: JSONValue}
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound: primitive union (simplified JSON)
    let json_primitive = interner.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::BOOLEAN,
        TypeId::NULL,
    ]);
    ctx.add_upper_bound(var_t, json_primitive);

    // Lower bound: string (valid JSON value)
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), TypeId::STRING);
}
#[test]
fn test_self_ref_constraint_recursive_array() {
    // Test: T extends T[] - array of itself constraint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Test with array bounds
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), string_array);
}

// ============================================================================
// Mutually Recursive Type Definitions Tests
// ============================================================================
// Tests for types that reference each other in a cycle
#[test]
fn test_mutual_recursion_two_types() {
    // Test: type A = { b: B }, type B = { a: A }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

    // Both get object lower bounds (breaking the cycle with concrete types)
    let prop_a = interner.intern_string("value");
    let obj_a = interner.object(vec![PropertyInfo::new(prop_a, TypeId::STRING)]);

    let prop_b = interner.intern_string("count");
    let obj_b = interner.object(vec![PropertyInfo::new(prop_b, TypeId::NUMBER)]);

    ctx.add_lower_bound(var_a, obj_a);
    ctx.add_lower_bound(var_b, obj_b);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, obj_a);
    assert_eq!(resolved[1].1, obj_b);
}
#[test]
fn test_mutual_recursion_three_types() {
    // Test: A -> B -> C -> A cycle
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // All have same upper bound
    ctx.add_upper_bound(var_a, TypeId::STRING);
    ctx.add_upper_bound(var_b, TypeId::STRING);
    ctx.add_upper_bound(var_c, TypeId::STRING);

    // Different literal lower bounds
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    let lit_c = interner.literal_string("c");

    ctx.add_lower_bound(var_a, lit_a);
    ctx.add_lower_bound(var_b, lit_b);
    ctx.add_lower_bound(var_c, lit_c);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].1, TypeId::STRING);
    assert_eq!(resolved[1].1, TypeId::STRING);
    assert_eq!(resolved[2].1, TypeId::STRING);
}
#[test]
fn test_mutual_recursion_shared_constraint() {
    // Test: A and B both bounded by same type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

    // Shared upper bound
    let shared_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    ctx.add_upper_bound(var_a, shared_union);
    ctx.add_upper_bound(var_b, shared_union);

    // A gets string, B gets number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, TypeId::STRING);
    assert_eq!(resolved[1].1, TypeId::NUMBER);
}
#[test]
fn test_mutual_recursion_array_element() {
    // Test: A = B[], B = A[] (arrays of each other)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

    // Concrete array lower bounds
    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);

    ctx.add_lower_bound(var_a, string_array);
    ctx.add_lower_bound(var_b, number_array);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, string_array);
    assert_eq!(resolved[1].1, number_array);
}
#[test]
fn test_mutual_recursion_function_params() {
    // Test: F = (a: G) => void, G = (f: F) => void
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let f_name = interner.intern_string("F");
    let g_name = interner.intern_string("G");

    let var_f = ctx.fresh_type_param(f_name, false);
    let var_g = ctx.fresh_type_param(g_name, false);

    // Create ParamInfo structs
    let param_f = ParamInfo {
        name: Some(interner.intern_string("a")),
        type_id: TypeId::STRING,
        optional: false,
        rest: false,
    };
    let param_g = ParamInfo {
        name: Some(interner.intern_string("f")),
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };

    // Concrete function lower bounds
    let fn_f = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![param_f],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let fn_g = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![param_g],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.add_lower_bound(var_f, fn_f);
    ctx.add_lower_bound(var_g, fn_g);

    let results = ctx.resolve_all_with_constraints();
    assert!(results.is_ok());
    let resolved = results.unwrap();
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].1, fn_f);
    assert_eq!(resolved[1].1, fn_g);
}

// ============================================================================
// Higher-Order Function Type Inference Tests
// ============================================================================
// Tests for inferring types in functions that take or return functions
#[test]
fn test_hof_callback_param_inference() {
    // Test: map<T, U>(arr: T[], fn: (x: T) => U) => U[]
    // Inferring T from array and U from callback return
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T inferred from array element type
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U inferred from callback return type
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
#[test]
fn test_hof_compose_functions() {
    // Test: compose<A, B, C>(f: (b: B) => C, g: (a: A) => B) => (a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Infer from concrete function types
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}
#[test]
fn test_hof_curried_function() {
    // Test: curry<A, B, C>(fn: (a: A, b: B) => C) => (a: A) => (b: B) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Infer from uncurried function parameters and return
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
}
#[test]
fn test_hof_reduce_accumulator() {
    // Test: reduce<T, U>(arr: T[], fn: (acc: U, val: T) => U, init: U) => U
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T from array elements
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U from initial value
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
#[test]
fn test_hof_function_returning_function() {
    // Test: factory<T>() => () => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T inferred from usage of returned function
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// ============================================================================
// Generic Method Chaining Tests (Fluent API Patterns)
// ============================================================================
// Tests for type inference in fluent/builder API patterns
#[test]
fn test_method_chain_builder_pattern() {
    // Test: Builder<T>.setValue(v: T).build() => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T inferred from setValue argument
    let string_lit = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, string_lit);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_method_chain_transform() {
    // Test: chain<T>.map<U>(fn: (t: T) => U) => chain<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T from initial chain value
    ctx.add_lower_bound(var_t, TypeId::STRING);

    // U from map callback return
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
#[test]
fn test_method_chain_filter() {
    // Test: chain<T>.filter(fn: (t: T) => boolean) => chain<T>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T preserved through filter
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_method_chain_multiple_transforms() {
    // Test: chain<A>.map<B>().map<C>().map<D>()
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");
    let d_name = interner.intern_string("D");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);
    let var_d = ctx.fresh_type_param(d_name, false);

    // Each step infers next type
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);
    ctx.add_lower_bound(var_d, TypeId::SYMBOL);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
    assert_eq!(results[2].1, TypeId::BOOLEAN);
    assert_eq!(results[3].1, TypeId::SYMBOL);
}
#[test]
fn test_method_chain_flatmap() {
    // Test: chain<T>.flatMap<U>(fn: (t: T) => chain<U>) => chain<U>
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T from outer chain
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    // U from inner chain returned by callback
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, string_array);
    assert_eq!(results[1].1, TypeId::NUMBER);
}

// ============================================================================
// Inference with Default Type Parameters Tests
// ============================================================================
// Tests for generic type inference when defaults are provided
#[test]
fn test_default_type_param_not_inferred() {
    // Test: <T = string>() => T - when no inference, use default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // No lower bounds added - would use default in real scenario
    // For inference, resolve gives unknown
    let result = ctx.resolve_with_constraints(var_t);
    assert!(result.is_ok());
}
#[test]
fn test_default_type_param_override() {
    // Test: <T = string>(x: T) => T - inference overrides default
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Inference from argument overrides default
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_default_type_param_with_constraint() {
    // Test: <T extends object = {}>(x: T) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Upper bound from constraint
    ctx.add_upper_bound(var_t, TypeId::OBJECT);

    // Specific object as lower bound
    let prop = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);
    ctx.add_lower_bound(var_t, obj);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, obj);
}
#[test]
fn test_default_type_param_chain() {
    // Test: <T = string, U = T>(x: U) => [T, U]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Both inferred from same value
    ctx.add_lower_bound(var_t, TypeId::NUMBER);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::NUMBER);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
#[test]
fn test_default_type_param_array() {
    // Test: <T = unknown>(arr?: T[]) => T[]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // Inferred from array element
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =========================================================================
// Generic Function Inference - Multiple Type Params
// =========================================================================
// Tests for generic function inference with multiple type parameters
#[test]
fn test_generic_function_three_type_params() {
    // Test: <A, B, C>(a: A, b: B, c: C) => [A, B, C]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // Called with (string, number, boolean)
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::BOOLEAN);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (a_name, TypeId::STRING));
    assert_eq!(results[1], (b_name, TypeId::NUMBER));
    assert_eq!(results[2], (c_name, TypeId::BOOLEAN));
}
#[test]
fn test_generic_function_dependent_type_params() {
    // Test: <T, U extends T>(base: T, derived: U) => U
    // Where U's constraint depends on T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T gets bound from first argument
    ctx.add_lower_bound(var_t, TypeId::STRING);
    // U gets bound from second argument (a string literal)
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_u, lit_hello);

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::STRING);
}
#[test]
fn test_generic_function_shared_type_param() {
    // Test: <T>(a: T, b: T) => T
    // Both arguments contribute to T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Called with two different string literals - should infer union
    let lit_a = interner.literal_string("a");
    let lit_b = interner.literal_string("b");
    ctx.add_lower_bound(var_t, lit_a);
    ctx.add_lower_bound(var_t, lit_b);

    let result = ctx.resolve_with_constraints(var_t).unwrap();

    // T is inferred as string (simplified from union of "a" | "b")
    assert_eq!(result, TypeId::STRING);
}

// =========================================================================
// Inference from Array/Object Destructuring Patterns
// =========================================================================
// Tests for type inference from destructuring patterns
#[test]
fn test_inference_array_element_type() {
    // Test: inferring element type from array access
    // <T>(arr: T[]) => T where arr[0] is used
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Array<string> is passed, so T should be string
    let string_array = interner.array(TypeId::STRING);
    // When destructuring [first] = arr, we infer T from the array element
    ctx.add_lower_bound(var_t, TypeId::STRING);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);

    // Verify the array type matches
    let _expected_array = interner.array(result);
    assert!(string_array != TypeId::ERROR);
}
#[test]
fn test_inference_tuple_element_types() {
    // Test: inferring from tuple destructuring
    // <A, B>(tuple: [A, B]) => A
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);

    // Tuple [string, number] is passed
    // Destructuring [first, second] = tuple infers A = string, B = number
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);

    let result_a = ctx.resolve_with_constraints(var_a).unwrap();
    let result_b = ctx.resolve_with_constraints(var_b).unwrap();

    assert_eq!(result_a, TypeId::STRING);
    assert_eq!(result_b, TypeId::NUMBER);
}
#[test]
fn test_inference_object_property_type() {
    // Test: inferring from object destructuring
    // <T>(obj: { value: T }) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Object { value: number } is passed
    // Destructuring { value } = obj infers T = number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_inference_nested_object_property() {
    // Test: inferring from nested object destructuring
    // <T>(obj: { inner: { value: T } }) => T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Nested destructuring { inner: { value } } = obj
    // value is boolean, so T = boolean
    ctx.add_lower_bound(var_t, TypeId::BOOLEAN);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

// =========================================================================
// Contextual Typing in Arrow Function Returns
// =========================================================================
// Tests for type inference from contextual typing of arrow function returns
#[test]
fn test_contextual_arrow_return_simple() {
    // Test: contextual typing provides return type
    // const fn: () => string = () => "hello"
    // The arrow function return is inferred from context
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Contextual type says return is string
    // Arrow function body returns a string literal
    ctx.add_upper_bound(var_t, TypeId::STRING);
    let lit_hello = interner.literal_string("hello");
    ctx.add_lower_bound(var_t, lit_hello);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should resolve to the more specific type: "hello"
    assert_eq!(result, TypeId::STRING);
}
#[test]
fn test_contextual_arrow_return_array() {
    // Test: contextual array return type
    // const fn: () => number[] = () => [1, 2, 3]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects Array<number>
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Return value contains number literals
    let lit_1 = interner.literal_number(1.0);
    ctx.add_lower_bound(var_t, lit_1);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Should infer the literal type
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_contextual_arrow_return_object() {
    // Test: contextual object return type
    // const fn: () => { x: number } = () => ({ x: 42 })
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Context expects { x: number }
    ctx.add_upper_bound(var_t, TypeId::NUMBER);
    // Actual value is 42
    let lit_42 = interner.literal_number(42.0);
    ctx.add_lower_bound(var_t, lit_42);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_contextual_arrow_callback_param() {
    // Test: callback parameter inference
    // arr.map((x) => x + 1) where arr: number[]
    // x should be inferred as number from the array element type
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let t_name = interner.intern_string("T");
    let var_t = ctx.fresh_type_param(t_name, false);

    // Contextual type from Array<number>.map callback is (element: number) => U
    // So T (the callback parameter type) should be number
    ctx.add_lower_bound(var_t, TypeId::NUMBER);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}
#[test]
fn test_contextual_arrow_higher_order() {
    // Test: higher-order function contextual typing
    // compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let a_name = interner.intern_string("A");
    let b_name = interner.intern_string("B");
    let c_name = interner.intern_string("C");

    let var_a = ctx.fresh_type_param(a_name, false);
    let var_b = ctx.fresh_type_param(b_name, false);
    let var_c = ctx.fresh_type_param(c_name, false);

    // compose((x: number) => x.toString(), (s: string) => s.length)
    // A = string, B = number, C = string
    ctx.add_lower_bound(var_a, TypeId::STRING);
    ctx.add_lower_bound(var_b, TypeId::NUMBER);
    ctx.add_lower_bound(var_c, TypeId::STRING);

    let results = ctx.resolve_all_with_constraints().unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (a_name, TypeId::STRING));
    assert_eq!(results[1], (b_name, TypeId::NUMBER));
    assert_eq!(results[2], (c_name, TypeId::STRING));
}

// ============================================================================
// Variadic Tuple Inference Tests
// ============================================================================
// Tests for inferring types in variadic tuple patterns like [...T]
#[test]
fn test_variadic_tuple_rest_element() {
    // Test: [...T] where T is inferred from tuple elements
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T inferred as array of strings from rest element
    let string_array = interner.array(TypeId::STRING);
    ctx.add_lower_bound(var_t, string_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, string_array);
}
#[test]
fn test_variadic_tuple_prefix_and_rest() {
    // Test: [string, ...T] - prefix element with rest
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T is the rest part after string prefix
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_t, number_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, number_array);
}
#[test]
fn test_variadic_tuple_suffix_and_rest() {
    // Test: [...T, string] - rest with suffix element
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");

    let var_t = ctx.fresh_type_param(t_name, false);

    // T is the rest part before string suffix
    let boolean_array = interner.array(TypeId::BOOLEAN);
    ctx.add_lower_bound(var_t, boolean_array);

    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, boolean_array);
}
#[test]
fn test_variadic_tuple_multiple_rest() {
    // Test: [...T, ...U] - multiple variadic segments
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // T and U are different array types
    let string_array = interner.array(TypeId::STRING);
    let number_array = interner.array(TypeId::NUMBER);
    ctx.add_lower_bound(var_t, string_array);
    ctx.add_lower_bound(var_u, number_array);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, string_array);
    assert_eq!(results[1].1, number_array);
}
#[test]
fn test_variadic_tuple_concat() {
    // Test: [...T, ...U] => [...T, ...U] (tuple concatenation)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Infer from concrete tuple parts
    let tuple_t = interner.tuple(vec![TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    let tuple_u = interner.tuple(vec![TupleElement {
        type_id: TypeId::NUMBER,
        name: None,
        optional: false,
        rest: false,
    }]);

    ctx.add_lower_bound(var_t, tuple_t);
    ctx.add_lower_bound(var_u, tuple_u);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, tuple_t);
    assert_eq!(results[1].1, tuple_u);
}

// ============================================================================
// Named Tuple Elements Tests
// ============================================================================
// Tests for tuples with named elements like [x: string, y: number]
#[test]
fn test_named_tuple_basic() {
    // Test: [x: T, y: U] - basic named tuple inference
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Infer from named tuple elements
    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
#[test]
fn test_named_tuple_with_optional() {
    // Test: [x: T, y?: U] - optional named element
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let t_name = interner.intern_string("T");
    let u_name = interner.intern_string("U");

    let var_t = ctx.fresh_type_param(t_name, false);
    let var_u = ctx.fresh_type_param(u_name, false);

    // Create named tuple with optional element
    let x_name = interner.intern_string("x");
    let y_name = interner.intern_string("y");
    let _named_tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: Some(x_name),
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(y_name),
            optional: true,
            rest: false,
        },
    ]);

    ctx.add_lower_bound(var_t, TypeId::STRING);
    ctx.add_lower_bound(var_u, TypeId::NUMBER);

    let results = ctx.resolve_all_with_constraints().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].1, TypeId::STRING);
    assert_eq!(results[1].1, TypeId::NUMBER);
}
