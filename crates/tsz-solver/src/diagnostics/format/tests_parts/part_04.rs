// A trailing rest parameter typed via a type *alias* or *generic application*
// resolves to its underlying tuple and expands like an inline tuple, matching
// tsc's `signatureToString` (`type R = [a, b]; ...rest: R` renders like the
// tuple). The formatter reads the alias body / generic arguments through its
// definition store.
fn format_rest_typed_with_store(
    db: &TypeInterner,
    def_store: &crate::def::DefinitionStore,
    rest_name: &str,
    rest_type_id: TypeId,
) -> String {
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string(rest_name)),
            type_id: rest_type_id,
            optional: false,
            rest: true,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt = TypeFormatter::new(db).with_def_store(def_store);
    fmt.format(func).into_owned()
}

fn unnamed_tuple_el(type_id: TypeId) -> crate::types::TupleElement {
    crate::types::TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

#[test]
fn format_rest_alias_tuple_param_expands() {
    // `type R = [string, number]; declare const f: (...rest: R) => void`
    // renders `(rest_0: string, rest_1: number)` — tsc resolves the alias to its
    // tuple before expanding.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        unnamed_tuple_el(TypeId::STRING),
        unnamed_tuple_el(TypeId::NUMBER),
    ]);
    let r_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        tuple,
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(r_def)),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_alias_tuple_param_preserves_labels() {
    // Named tuple members in the alias win over the `{name}_{i}` fallback:
    // `type R = [a: string, b: number]; ...rest: R` -> `(a: string, b: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: Some(db.intern_string("a")),
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(db.intern_string("b")),
            optional: false,
            rest: false,
        },
    ]);
    // A renamed binder (`Coords`) proves the resolution is structural, not
    // keyed on any particular alias name.
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Coords"),
        vec![],
        tuple,
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(def)),
        "(a: string, b: number) => void"
    );
}

#[test]
fn format_rest_nested_alias_tuple_param_expands() {
    // `type Inner = [string, number]; type R = Inner; ...rest: R` expands
    // through the alias chain to `(rest_0: string, rest_1: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let tuple = db.tuple(vec![
        unnamed_tuple_el(TypeId::STRING),
        unnamed_tuple_el(TypeId::NUMBER),
    ]);
    let inner_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Inner"),
        vec![],
        tuple,
    ));
    let outer_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        db.lazy(inner_def),
    ));
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(outer_def)),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_generic_alias_application_expands() {
    // `type Pair<T> = [T, number]; ...rest: Pair<string>` instantiates to
    // `[string, number]` and expands to `(rest_0: string, rest_1: number)`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t = db.type_param(t_param);
    let body = db.tuple(vec![unnamed_tuple_el(t), unnamed_tuple_el(TypeId::NUMBER)]);
    let pair_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Pair"),
        vec![t_param],
        body,
    ));
    def_store.register_type_to_def(body, pair_def);
    let app = db.application(db.lazy(pair_def), vec![TypeId::STRING]);
    assert_eq!(
        format_rest_typed_with_store(&db, &def_store, "rest", app),
        "(rest_0: string, rest_1: number) => void"
    );
}

#[test]
fn format_rest_array_alias_param_not_expanded() {
    // `type R = number[]; ...rest: R` is not a tuple, so tsc keeps the written
    // alias form (`...rest: R`) rather than expanding.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let arr = db.array(TypeId::NUMBER);
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("R"),
        vec![],
        arr,
    ));
    let result = format_rest_typed_with_store(&db, &def_store, "rest", db.lazy(def));
    assert!(
        result.starts_with("(...rest:") && !result.contains("rest_0"),
        "an array-bodied alias rest must stay unexpanded, got: {result}"
    );
}

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
        params: vec![ParamInfo {
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
    assert_eq!(result, "[string, (number | undefined)?]");
}
