use super::*;

#[test]
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_zero_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1Ee0");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_mixed_case_exponent_double_sign_trailing() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1Ee++");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1E+");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_minus_missing_digits() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1E-");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_double_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1E++1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_uppercase_exponent_double_minus() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1E--1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_negative() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e-0001");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_positive() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e+0001");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_exponent_leading_zeros_without_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e0001");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_missing_exponent_sign() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("1e21");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_leading_zero_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("01.0");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_hex_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("0x1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_binary_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("0b1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_octal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("0o7");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_exponent_leading_zero_mantissa() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("01e+1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_leading_dot_decimal_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string(".5");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_multiple_leading_zeros() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("00");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
fn test_resolve_bounds_number_index_ignores_negative_hex_name() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let var = ctx.fresh_type_param(interner.intern_string("T"), false);
    let name = interner.intern_string("-0x1");

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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
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
            is_symbol_named: false,
            single_quoted_name: false,
        }],
        string_index: None,
        number_index: None,
    });

    ctx.add_lower_bound(var, lower_type);
    ctx.add_upper_bound(var, upper_type);

    let result = ctx.resolve_with_constraints(var).unwrap();
    assert_eq!(result, lower_type);
}
