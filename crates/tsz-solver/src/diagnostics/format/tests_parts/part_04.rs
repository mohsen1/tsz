// Split out of part_00.rs to hold the format module render tests under the
// repo-wide 2000-LOC cap. Included by tests.rs alongside the sibling parts;
// all parts share one module scope via include!.

#[test]
fn format_overload_renamed_type_param_uses_display_name() {
    // A type parameter renamed to the synthetic `__overload_sig_*` atom for
    // name-keyed overload inference must render its original declared name and
    // never leak the placeholder into a diagnostic.
    let db = TypeInterner::new();
    let synthetic = db.intern_string("__overload_sig_2_tp_0");
    let display = db.intern_string("Value");
    let tp = db.type_param(TypeParamInfo {
        name: synthetic,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::OverloadRenamed {
            display_name: display,
        },
    });
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: tp,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(func);
    assert_eq!(result, "(x: Value) => void");
    assert!(
        !result.contains("__overload_sig"),
        "synthetic overload placeholder leaked: {result}"
    );
}

#[test]
fn format_function_with_type_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(result.contains("<T>"), "Expected type param, got: {result}");
    assert!(result.contains("x: T"));
    assert!(result.contains("=> T"));
}

#[test]
fn format_function_type_param_with_constraint() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("T extends string"),
        "Expected 'T extends string', got: {result}"
    );
}

#[test]
fn format_function_type_param_with_structural_array_constraint_uses_shorthand() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let constraint = db.array(TypeId::ANY);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends any[]>"),
        "Expected structural array constraint shorthand, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_non_primitive_array_constraint_uses_generic_form() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let foo = PropertyInfo::new(db.intern_string("foo"), TypeId::STRING);
    let object = db.object(vec![foo]);
    let constraint = db.array(object);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends Array<{ foo: string; }>>"),
        "Expected non-primitive array constraint to preserve generic form, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_array_application_constraint_preserves_generic_form() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let array_name = db.unresolved_type_name(db.intern_string("Array"));
    let constraint = db.application(array_name, vec![TypeId::ANY]);
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: Some(constraint),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(constraint),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = fmt.format(func);
    assert!(
        result.contains("<T extends Array<any>>"),
        "Expected explicit Array<T> constraint syntax to be preserved, got: {result}"
    );
}

#[test]
fn format_function_type_param_with_default() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: Some(TypeId::STRING),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        params: vec![ParamInfo { suppress_display_optional: false,
            name: Some(db.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("T = string"),
        "Expected 'T = string', got: {result}"
    );
}

#[test]
fn format_constructor_function() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("new "),
        "Constructor should start with 'new', got: {result}"
    );
}

// =================================================================
// Array/tuple formatting
// =================================================================

#[test]
fn format_array_primitive() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(db.array(TypeId::STRING)), "string[]");
    assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
    assert_eq!(fmt.format(db.array(TypeId::BOOLEAN)), "boolean[]");
}

#[test]
fn format_array_of_function_parenthesized() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let arr = db.array(func);
    let result = fmt.format(arr);
    assert!(
        result.starts_with('(') && result.ends_with(")[]"),
        "Array of function should be parenthesized, got: {result}"
    );
}

#[test]
fn format_tuple_empty() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![]);
    assert_eq!(fmt.format(tuple), "[]");
}

#[test]
fn format_tuple_single_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![crate::types::TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    }]);
    assert_eq!(fmt.format(tuple), "[string]");
}

#[test]
fn format_tuple_two_elements() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(fmt.format(tuple), "[string, number]");
}

#[test]
fn format_tuple_named_elements() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(db.intern_string("name")),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(db.intern_string("age")),
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(fmt.format(tuple), "[name: string, age: number]");
}

#[test]
fn format_tuple_optional_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: true,
            rest: false,
        },
    ]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[string, (number | undefined)?]");
}
