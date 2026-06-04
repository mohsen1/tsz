#[test]
fn test_infer_generic_number_index_from_numeric_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let indexed_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("bag")),
            type_id: indexed_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let object_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("0"),
        TypeId::STRING,
    )]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[object_literal]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_generic_number_index_ignores_noncanonical_numeric_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let indexed_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("bag")),
            type_id: indexed_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let object_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("01"),
        TypeId::STRING,
    )]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[object_literal]);
    // Non-canonical numeric property "01" doesn't match number index;
    // uninferred type param resolves to unknown, not error.
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_infer_generic_number_index_ignores_negative_zero_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let indexed_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("bag")),
            type_id: indexed_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let object_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("-0"),
        TypeId::STRING,
    )]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[object_literal]);
    // Non-canonical numeric property "-0" doesn't match number index;
    // uninferred type param resolves to unknown, not error.
    assert_eq!(result, TypeId::UNKNOWN);
}

#[test]
fn test_infer_generic_number_index_from_nan_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let indexed_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("bag")),
            type_id: indexed_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let object_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("NaN"),
        TypeId::STRING,
    )]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[object_literal]);
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_infer_generic_number_index_from_exponent_property() {
    let interner = TypeInterner::new();
    let mut subtype = CompatChecker::new(&interner);

    let t_param = TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = interner.intern(TypeData::TypeParameter(t_param));

    let indexed_t = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: t_type,
            readonly: false,
            param_name: None,
        }),
    });

    let func = FunctionShape {
        type_params: vec![t_param],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("bag")),
            type_id: indexed_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };

    let object_literal = interner.object(vec![PropertyInfo::new(
        interner.intern_string("1e-7"),
        TypeId::STRING,
    )]);

    let result = infer_generic_function(&interner, &mut subtype, &func, &[object_literal]);
    assert_eq!(result, TypeId::STRING);
}
