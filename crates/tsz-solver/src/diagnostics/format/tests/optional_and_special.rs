use super::*;

// =================================================================

#[test]
fn format_const_type_param() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t_atom = db.intern_string("T");
    let t_param = db.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: true,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: true,
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
        result.contains("const T"),
        "Expected 'const T' in type params, got: {result}"
    );
}

#[test]
fn generic_class_type_shows_type_params() {
    // When a generic class (e.g., `class B<T>`) has its instance type formatted,
    // the formatter should show `B<T>` not just `B`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Create an empty object type as the class instance body
    let instance_type = db.object(vec![]);

    // Register a class definition with one type parameter T
    let name = db.intern_string("B");
    let t_name = db.intern_string("T");
    let info = crate::def::DefinitionInfo {
        kind: crate::def::DefKind::Class,
        name,
        type_params: vec![TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        }],
        body: Some(instance_type),
        instance_shape: None,
        static_shape: None,
        extends: None,
        implements: Vec::new(),
        enum_members: Vec::new(),
        exports: Vec::new(),
        span: None,
        file_id: None,
        symbol_id: None,
        heritage_names: Vec::new(),
        is_abstract: false,
        is_const: false,
        is_exported: false,
        is_global_augmentation: false,
        is_declare: false,
    };
    let def_id = def_store.register(info);

    // Register the instance type -> def mapping
    def_store.register_type_to_def(instance_type, def_id);

    // Without def_store: should show structural form
    let mut fmt = TypeFormatter::new(&db);
    let without = fmt.format(instance_type);
    assert_eq!(without, "{}");

    // With def_store: should show `B<T>` with type parameter name
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let with = fmt.format(instance_type);
    assert_eq!(with, "B<T>", "Generic class should show type params");
}

#[test]
fn application_lazy_shows_type_args() {
    // Application(Lazy(def_id), [string, number]) should format as `Name<string, number>`
    use crate::caches::db::QueryDatabase;
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Register a definition
    let name = db.intern_string("MyClass");
    let info = crate::def::DefinitionInfo {
        kind: crate::def::DefKind::Class,
        name,
        type_params: vec![
            TypeParamInfo {
                name: db.intern_string("T"),
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("U"),
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        body: None,
        instance_shape: None,
        static_shape: None,
        extends: None,
        implements: Vec::new(),
        enum_members: Vec::new(),
        exports: Vec::new(),
        span: None,
        file_id: None,
        symbol_id: None,
        heritage_names: Vec::new(),
        is_abstract: false,
        is_const: false,
        is_exported: false,
        is_global_augmentation: false,
        is_declare: false,
    };
    let def_id = def_store.register(info);

    // Create Application(Lazy(def_id), [string, number])
    let factory = db.factory();
    let lazy = factory.lazy(def_id);
    let app = factory.application(lazy, vec![TypeId::STRING, TypeId::NUMBER]);

    // With def_store: should show `MyClass<string, number>`
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(app);
    assert_eq!(
        result, "MyClass<string, number>",
        "Application should show formatted type args"
    );
}
// NOTE: lazy_raw_def_id_falls_back_to_symbol_name was removed.
// DefId and SymbolId are independent ID spaces. The raw-value fallback
// was removed in bfd1e1ad05 because it caused incorrect type names
// (e.g., enum "Foo" displaying as "timeout" when IDs collided).

// =================================================================
// Optional parameter/property display (no redundant `| undefined`)
// =================================================================

#[test]
fn optional_param_shows_undefined() {
    // The formatter displays whatever type is stored in ParamInfo.type_id.
    // The checker is responsible for adding `| undefined` to `?`-optional
    // params before storing them.  When the stored type is plain `string`,
    // the formatter shows `(a?: string)`.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("a")),
            type_id: TypeId::STRING,
            optional: true,
            rest: false,
        }],
        return_type: TypeId::ANY,
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(
        result, "(a?: string) => any",
        "Formatter displays stored type as-is; checker adds | undefined for ?-optional"
    );
}

#[test]
fn optional_param_with_union_undefined_keeps_it() {
    // When the type is internally `string | undefined`, the formatter keeps
    // `undefined` for optional params — matches tsc behavior.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("a")),
            type_id: str_or_undef,
            optional: true,
            rest: false,
        }],
        return_type: TypeId::ANY,
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(
        result, "(a?: string | undefined) => any",
        "Optional param preserves '| undefined' — matches tsc display"
    );
}

#[test]
fn optional_property_shows_undefined() {
    // tsc: `{ x?: string | undefined; }` — object properties show | undefined
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: crate::types::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let result = fmt.format(obj);
    assert_eq!(
        result, "{ x?: string | undefined; }",
        "tsc shows '| undefined' for optional object properties"
    );
}

#[test]
fn optional_property_never_shows_as_undefined() {
    // When the property type is `never` and it's optional, tsc displays just `undefined`
    // since `never | undefined = undefined`.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let obj = db.object(vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: TypeId::NEVER,
        write_type: TypeId::NEVER,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: crate::types::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let result = fmt.format(obj);
    assert_eq!(
        result, "{ x?: undefined; }",
        "Optional never property displays as undefined, not 'never | undefined'"
    );
}

#[test]
fn optional_property_with_union_undefined_keeps_it() {
    // When the type already has `string | undefined`, display as-is (no duplicate)
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let obj = db.object(vec![PropertyInfo {
        name: db.intern_string("x"),
        type_id: str_or_undef,
        write_type: str_or_undef,
        optional: true,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: crate::types::Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);
    let result = fmt.format(obj);
    assert_eq!(
        result, "{ x?: string | undefined; }",
        "Optional property with string | undefined should keep it as-is"
    );
}

#[test]
fn empty_object_shape_formats_without_spurious_separator() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(db.object(Vec::new())), "{}");
}

#[test]
fn non_optional_param_keeps_undefined_in_union() {
    // Non-optional params should still show `| undefined` if it's in the type
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("a")),
            type_id: str_or_undef,
            optional: false,
            rest: false,
        }],
        return_type: TypeId::ANY,
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let result = fmt.format(func);
    assert_eq!(
        result, "(a: string | undefined) => any",
        "Non-optional param should keep '| undefined' in union"
    );
}

#[test]
fn object_union_optionalization_in_default_mode() {
    // Without diagnostic mode, object unions should show synthetic `?: undefined`
    // for properties that exist in some but not all union members.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create { prop: string; anotherP: string; } | { prop: number; }
    let obj1 = db.object(vec![
        PropertyInfo::new(db.intern_string("prop"), TypeId::STRING),
        PropertyInfo::new(db.intern_string("anotherP"), TypeId::STRING),
    ]);
    let obj2 = db.object(vec![PropertyInfo::new(
        db.intern_string("prop"),
        TypeId::NUMBER,
    )]);
    let union_id = db.union_preserve_members(vec![obj1, obj2]);

    let result = fmt.format(union_id);
    // In default (hover/quickinfo) mode, we add synthetic `anotherP?: undefined`
    // to the second member for better discriminated union display.
    assert!(
        result.contains("anotherP?:"),
        "Default mode should add synthetic optional properties, got: {result}"
    );
}

#[test]
fn object_union_no_optionalization_in_diagnostic_mode() {
    // With diagnostic mode, object unions should NOT add synthetic properties.
    // tsc only uses optionalization in hover/quickinfo, not in error messages.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db).with_diagnostic_mode();

    // Create { prop: string; anotherP: string; } | { prop: number; }
    let obj1 = db.object(vec![
        PropertyInfo::new(db.intern_string("prop"), TypeId::STRING),
        PropertyInfo::new(db.intern_string("anotherP"), TypeId::STRING),
    ]);
    let obj2 = db.object(vec![PropertyInfo::new(
        db.intern_string("prop"),
        TypeId::NUMBER,
    )]);
    let union_id = db.union_preserve_members(vec![obj1, obj2]);

    let result = fmt.format(union_id);
    // In diagnostic mode, we should NOT add synthetic properties.
    // The result should be: { prop: string; anotherP: string; } | { prop: number; }
    assert!(
        !result.contains("anotherP?:"),
        "Diagnostic mode should NOT add synthetic optional properties, got: {result}"
    );
    assert!(
        result.contains("{ prop: number; }"),
        "Second member should have original shape without synthetic props, got: {result}"
    );
}
