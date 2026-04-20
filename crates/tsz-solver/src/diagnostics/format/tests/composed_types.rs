use super::*;

// =================================================================
// Function type formatting
// =================================================================

#[test]
fn format_function_no_params() {
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
    let result = fmt.format(func);
    assert_eq!(result, "() => void");
}

#[test]
fn format_function_two_params() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(db.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(result, "(a: string, b: number) => boolean");
}

#[test]
fn format_function_rest_param() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let arr = db.array(TypeId::STRING);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("args")),
            type_id: arr,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert!(
        result.contains("...args"),
        "Expected rest param, got: {result}"
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
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
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
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        }],
        params: vec![ParamInfo {
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
fn format_function_type_param_with_default() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: Some(TypeId::STRING),
        is_const: false,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: Some(TypeId::STRING),
            is_const: false,
        }],
        params: vec![ParamInfo {
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
    assert_eq!(result, "[string, number?]");
}

#[test]
fn format_tuple_rest_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let str_arr = db.array(TypeId::STRING);
    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: str_arr,
            name: None,
            optional: false,
            rest: true,
        },
    ]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[number, ...string[]]");
}

// =================================================================
// Conditional type formatting
// =================================================================

#[test]
fn format_conditional_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let cond = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let result = fmt.format(cond);
    assert_eq!(result, "string extends number ? boolean : never");
}

#[test]
fn format_conditional_type_nested() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // T extends string ? (T extends "a" ? 1 : 2) : 3
    let inner = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: db.literal_string("a"),
        true_type: db.literal_number(1.0),
        false_type: db.literal_number(2.0),
        is_distributive: false,
    });
    let outer = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: inner,
        false_type: db.literal_number(3.0),
        is_distributive: false,
    });
    let result = fmt.format(outer);
    assert!(result.contains("extends"));
    assert!(result.contains("?"));
    assert!(result.contains(":"));
}
