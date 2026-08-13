#[test]
fn format_callable_multiple_call_signatures() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let callable = db.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let result = fmt.format(callable);
    // Multiple signatures => object-like format with { sig1; sig2 }
    assert!(
        result.contains("{") && result.contains("}"),
        "Multiple sigs should use object format, got: {result}"
    );
}

#[test]
fn format_callable_displays_zero_arg_overload_first() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let callable = db.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("locales")),
                    type_id: TypeId::STRING,
                    optional: true,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let result = fmt.format(callable);
    let zero_pos = result
        .find("(): string")
        .expect("expected zero-arg overload");
    let opt_pos = result
        .find("(locales?: string): string")
        .expect("expected optional-arg overload");
    assert!(
        zero_pos < opt_pos,
        "Expected zero-arg overload to display first, got: {result}"
    );
}

#[test]
fn format_construct_only_interface_callable_uses_type_name() {
    let db = TypeInterner::new();
    let mut symbols = tsz_binder::SymbolArena::new();
    let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "ConstructableA".into());

    let callable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: Some(sym_id),
        is_abstract: false,
    });

    let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
    assert_eq!(fmt.format(callable), "ConstructableA");
}

// =================================================================
// Recursive / BoundParameter formatting
// =================================================================

#[test]
fn format_recursive_index() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let rec = db.recursive(0);
    assert_eq!(fmt.format(rec), "Recursive(0)");

    let rec2 = db.recursive(3);
    assert_eq!(fmt.format(rec2), "Recursive(3)");
}

#[test]
fn format_bound_parameter() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let bp = db.bound_parameter(0);
    assert_eq!(fmt.format(bp), "BoundParameter(0)");

    let bp1 = db.bound_parameter(1);
    assert_eq!(fmt.format(bp1), "BoundParameter(1)");
}

// =================================================================
// Property name quoting edge cases
// =================================================================

#[test]
fn needs_property_name_quotes_edge_cases() {
    // Leading digit is not a valid identifier start
    assert!(super::needs_property_name_quotes("1abc"));
    // Underscore-only is valid
    assert!(!super::needs_property_name_quotes("_"));
    assert!(!super::needs_property_name_quotes("__proto__"));
    // Dollar-only
    assert!(!super::needs_property_name_quotes("$"));
    assert!(!super::needs_property_name_quotes("$0"));
    // Special characters
    assert!(super::needs_property_name_quotes("."));
    assert!(super::needs_property_name_quotes("@"));
    assert!(super::needs_property_name_quotes("#private"));
}

#[test]
fn needs_property_name_quotes_unicode_identifiers() {
    // Unicode identifier-start characters must NOT be quoted (ECMAScript parity)
    assert!(!super::needs_property_name_quotes("café"));
    assert!(!super::needs_property_name_quotes("naïve"));
    assert!(!super::needs_property_name_quotes("α"));
    assert!(!super::needs_property_name_quotes("αβγ"));
    assert!(!super::needs_property_name_quotes("日本語"));
    // Non-identifier Unicode still needs quotes
    assert!(super::needs_property_name_quotes("data-id"));
    assert!(super::needs_property_name_quotes("aria-label"));
}

#[test]
fn needs_property_name_quotes_canonical_numeric_forms() {
    // Canonical JS-numeric forms (matching `Number.prototype.toString()`
    // round-trip) are displayed without quotes by tsc in object literal
    // type display.
    assert!(!super::needs_property_name_quotes("3.14"));
    assert!(!super::needs_property_name_quotes("-1"));
    assert!(!super::needs_property_name_quotes("1e-7"));
    assert!(!super::needs_property_name_quotes("5.462437423415177e+244"));
    // `Infinity` / `-Infinity` are valid numeric literal names per
    // tsc's `isNumericLiteralName`, so they are also unquoted.
    assert!(!super::needs_property_name_quotes("Infinity"));
    assert!(!super::needs_property_name_quotes("-Infinity"));
    assert!(!super::needs_property_name_quotes("NaN"));
    // Non-canonical numeric forms still need quotes (they don't
    // round-trip through `Number.toString`). `01` starts with a digit and
    // is not a valid identifier; `1.` contains a non-identifier dot.
    assert!(super::needs_property_name_quotes("1."));
    assert!(super::needs_property_name_quotes("01"));
}

#[test]
fn needs_property_name_quotes_bracket_wrapped() {
    // Computed symbol property names wrapped in brackets should not be quoted
    assert!(!super::needs_property_name_quotes("[Symbol.asyncIterator]"));
    assert!(!super::needs_property_name_quotes("[Symbol.iterator]"));
    assert!(!super::needs_property_name_quotes("[Symbol.hasInstance]"));
    assert!(!super::needs_property_name_quotes("[Symbol.toPrimitive]"));
    // Single bracket only (not a computed property) should still need quotes
    assert!(super::needs_property_name_quotes("["));
    assert!(super::needs_property_name_quotes("]"));
    // Bracket at start but not end (not computed property syntax)
    assert!(super::needs_property_name_quotes("[foo"));
    assert!(super::needs_property_name_quotes("foo]"));
}

// =================================================================
// Method shorthand formatting
// =================================================================

#[test]
fn format_object_method_shorthand() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let method_type = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut method_prop = PropertyInfo::new(db.intern_string("greet"), method_type);
    method_prop.is_method = true;

    let obj = db.object(vec![method_prop]);
    let result = fmt.format(obj);
    // Method shorthand: greet(x: number): string
    assert!(
        result.contains("greet(") && result.contains("): string"),
        "Expected method shorthand, got: {result}"
    );
    // Should NOT use arrow notation
    assert!(
        !result.contains("=>"),
        "Method shorthand should use ':' not '=>', got: {result}"
    );
}

#[test]
fn format_object_readonly_method_uses_property_form() {
    // A readonly method member (e.g. captured by `as const`) must render as a
    // readonly property holding a function type — `readonly greet: (x: number) => string`
    // — never as a method signature. This mirrors tsc's node builder, which only emits a
    // method signature for a non-readonly method symbol.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let method_type = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut method_prop = PropertyInfo::new(db.intern_string("greet"), method_type);
    method_prop.is_method = true;
    method_prop.readonly = true;

    let obj = db.object(vec![method_prop]);
    let result = fmt.format(obj);
    // Property form with an arrow function type, not method shorthand.
    assert!(
        result.contains("readonly greet: (x: number) => string"),
        "Expected readonly property form, got: {result}"
    );
    assert!(
        !result.contains("greet("),
        "Readonly method must not use method shorthand, got: {result}"
    );
}

// =================================================================
// Const type parameter
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
        origin: crate::types::TypeParamOrigin::User,
    });
    let func = db.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: true,
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
        result.contains("const T"),
        "Expected 'const T' in type params, got: {result}"
    );
}

#[test]
fn generic_class_type_shows_type_params() {
    // When a generic class (e.g., `class B<T> { value: T }`) has its instance
    // type formatted, the formatter should show `B<T>`, not just `B` nor
    // the structural body.
    //
    // Note: we give the class body one property so its instance TypeId does
    // not intern to the universally-shared empty object `{}`. Real classes
    // and interfaces in the checker that happen to have zero properties still
    // render their structural form `{}` in diagnostics, matching tsc's
    // behaviour where user-written `{}` annotations stay `{}` regardless of
    // any alias whose body happens to reduce to the empty shape.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // One fake property so the instance TypeId is distinct from the
    // universally-shared empty object `{}`.
    let instance_type = db.object(vec![PropertyInfo::new(
        db.intern_string("value"),
        TypeId::STRING,
    )]);

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
            origin: crate::types::TypeParamOrigin::User,
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

    // Without def_store: should show structural form.
    let mut fmt = TypeFormatter::new(&db);
    let without = fmt.format(instance_type);
    assert_eq!(without, "{ value: string; }");

    // With def_store: should show `B<T>` with type parameter name.
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
                origin: crate::types::TypeParamOrigin::User,
            },
            TypeParamInfo {
                name: db.intern_string("U"),
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
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

#[test]
fn resolved_indexed_access_alias_bodies_stay_structural_without_repainting_written_aliases() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let data_fetch_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("DataFetchFns"),
        vec![],
        db.object(vec![]),
    ));

    let t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let f_param = TypeParamInfo {
        name: db.intern_string("F"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t = db.type_param(t_param);
    let f = db.type_param(f_param);

    let direct_body = db.index_access(db.index_access(db.lazy(data_fetch_def), t), f);
    let direct_alias_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("TypeHardcodedAsParameterWithoutReturnType"),
        vec![t_param, f_param],
        direct_body,
    ));
    def_store.register_type_to_def(direct_body, direct_alias_def);
    let direct_app = db.application(db.lazy(direct_alias_def), vec![t, f]);

    let vehicle_t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let vehicle_t = db.type_param(vehicle_t_param);
    let vehicle_body = db.index_access(db.lazy(data_fetch_def), vehicle_t);
    let vehicle_alias_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("VehicleSelector"),
        vec![vehicle_t_param],
        vehicle_body,
    ));
    def_store.register_type_to_def(vehicle_body, vehicle_alias_def);
    let vehicle_app = db.application(db.lazy(vehicle_alias_def), vec![t]);
    let nested_access = db.index_access(vehicle_app, f);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(direct_body),
        "DataFetchFns[T][F]",
        "Resolved indexed-access alias bodies should stay expanded"
    );
    assert_eq!(
        fmt.format(vehicle_body),
        "DataFetchFns[T]",
        "Resolved indexed-access helper aliases should stay expanded"
    );
    assert_eq!(
        fmt.format(direct_app),
        "TypeHardcodedAsParameterWithoutReturnType<T, F>",
        "Direct generic alias applications should preserve the alias name"
    );
    assert_eq!(
        fmt.format(nested_access),
        "VehicleSelector<T>[F]",
        "Nested indexed access should preserve the helper alias surface when it is written directly"
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
fn optional_param_shows_synthetic_undefined_when_surface_preservation_disabled() {
    // In diagnostics that choose synthetic parameter rendering, optional params
    // add `| undefined` when the stored type does not already include it.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db).with_preserve_optional_parameter_surface_syntax(false);

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
        result, "(a?: string | undefined) => any",
        "Assignability-mode rendering appends synthetic undefined for optional params"
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
        is_symbol_named: false,
        single_quoted_name: false, non_widening: false,
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
        is_symbol_named: false,
        single_quoted_name: false, non_widening: false,
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
        is_symbol_named: false,
        single_quoted_name: false, non_widening: false,
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

// =================================================================
// Distributive conditional alias display
// =================================================================
//
// When an alias of the form
//   type Foo<T> = T extends X ? A : B  (T naked → distributive)
// is applied to `boolean`, tsc distributes `boolean` as `true | false`
// and shows the fully evaluated branches in error messages — not the
// alias-application form (`Foo<boolean>`). The formatter mirrors that
// policy in `distributed_conditional_application_display`.

fn build_distributive_foo_alias(
    db: &TypeInterner,
    def_store: &crate::def::DefinitionStore,
) -> TypeId {
    let t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    };
    let t = db.type_param(t_param);

    // Foo<T> = T extends boolean ? { kind: 'b' } : { kind: 'o' }
    let true_branch = db.object(vec![PropertyInfo::new(
        db.intern_string("kind"),
        db.literal_string("b"),
    )]);
    let false_branch = db.object(vec![PropertyInfo::new(
        db.intern_string("kind"),
        db.literal_string("o"),
    )]);
    let cond = db.conditional(crate::types::ConditionalType {
        check_type: t,
        extends_type: TypeId::BOOLEAN,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    });
    let foo_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Foo"),
        vec![t_param],
        cond,
    ));
    db.lazy(foo_def)
}

#[test]
fn distributive_conditional_alias_with_boolean_renders_branches_not_alias() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let foo_lazy = build_distributive_foo_alias(&db, &def_store);

    // Application(Foo, [boolean])
    let app = db.application(foo_lazy, vec![TypeId::BOOLEAN]);
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(app);

    // tsc distributes `boolean` to `true | false`. Both branches evaluate
    // to `{ kind: "b" }` (true and false both extend boolean), so the
    // union normalizes to a single `{ kind: "b"; }` — not `Foo<boolean>`.
    assert!(
        !result.contains("Foo<boolean>"),
        "Distributive conditional applied to `boolean` should not display \
         as the alias-application form. Got: {result}"
    );
    assert!(
        result.contains("kind: \"b\""),
        "Distributed branches must be evaluated and rendered structurally. Got: {result}"
    );
}

#[test]
fn conditional_alias_application_resolving_to_object_renders_structurally() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let foo_lazy = build_distributive_foo_alias(&db, &def_store);

    // Application(Foo, [string]) — `string` is not a union, so the conditional
    // resolves (without distributing) to its false branch `{ kind: "o" }`.
    let app = db.application(foo_lazy, vec![TypeId::STRING]);
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(app);

    // A conditional-bodied alias application drops its alias symbol once the
    // conditional resolves: tsc 6.0.2 renders the resolved branch structurally
    // (`{ kind: "o"; }`), never `Foo<string>` (issue #10914).
    assert_eq!(
        result, "{ kind: \"o\"; }",
        "A resolved conditional application must render its branch structurally. Got: {result}"
    );
}

#[test]
fn conditional_alias_application_resolving_to_tuple_renders_structurally() {
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
    let tuple = db.tuple(vec![crate::types::TupleElement {
        type_id: t,
        name: None,
        optional: false,
        rest: false,
    }]);
    let cond = db.conditional(crate::types::ConditionalType {
        check_type: t,
        extends_type: TypeId::STRING,
        true_type: tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let tuple_box_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("TupleBox"),
        vec![t_param],
        cond,
    ));
    let app = db.application(db.lazy(tuple_box_def), vec![TypeId::STRING]);
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);

    // A conditional-bodied alias application drops its alias symbol once the
    // conditional resolves: tsc 6.0.2 renders the resolved branch structurally
    // (`TupleBox<string>` → `[string]`), never the `TupleBox<string>` surface.
    assert_eq!(
        fmt.format(app),
        "[string]",
        "A resolved conditional alias application renders its branch structurally \
         for any concrete shape, including tuples"
    );
}

// The Application display-reduction cascade is memoized per `Application`
// `TypeId` (#13480). Re-reaching the same reducible Application through a
// composite must produce byte-identical output to formatting it alone — the
// cache must not change the rendering, only avoid re-running the
// `instantiate_generic` + `evaluate_type` cascade. The binder is named `Wrap`
// (not `Foo`) so the assertion cannot accidentally depend on a specific alias
// identifier, per the anti-hardcoding rule.
#[test]
fn repeated_conditional_alias_application_renders_identically_when_memoized() {
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

    // Wrap<T> = T extends boolean ? { kind: "b" } : { kind: "o" }
    let true_branch = db.object(vec![PropertyInfo::new(
        db.intern_string("kind"),
        db.literal_string("b"),
    )]);
    let false_branch = db.object(vec![PropertyInfo::new(
        db.intern_string("kind"),
        db.literal_string("o"),
    )]);
    let cond = db.conditional(crate::types::ConditionalType {
        check_type: t,
        extends_type: TypeId::BOOLEAN,
        true_type: true_branch,
        false_type: false_branch,
        is_distributive: true,
    });
    let wrap_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Wrap"),
        vec![t_param],
        cond,
    ));

    // Application(Wrap, [string]) resolves (non-distributively) to the false
    // branch `{ kind: "o" }` and drops its alias symbol.
    let app = db.application(db.lazy(wrap_def), vec![TypeId::STRING]);

    // Baseline: format the application on its own.
    let solo = {
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        fmt.format(app)
    };
    assert_eq!(solo, "{ kind: \"o\"; }");

    // The same Application appears twice in a tuple: the second occurrence is a
    // cache hit. The reduced branch must render identically at both positions.
    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: app,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: app,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let composite = fmt.format(tuple);
    assert_eq!(
        composite,
        format!("[{solo}, {solo}]"),
        "Memoized reduction must render the shared Application identically at \
         every occurrence; got: {composite}"
    );
}

// =====================================================================
// Union containing a Lazy alias — TS2859 / general union-display parity
// =====================================================================
//
// When a user writes `T | null` where `T` is a type alias whose body is a
// union (e.g., `type T = "a" | "b" | undefined`), tsc displays the diagnostic
// with the alias name preserved at the top level: `T | null`. The flattened
// member list `"a" | "b" | undefined | null` is the structural form, but the
// printer is supposed to factor the alias back out for display.
//
// These tests lock in two invariants:
//   1. A union built from `[Lazy(T), null]` *without* prior flattening must
//      display as `T | null` (the Lazy is preserved).
//   2. After we add union-origin tracking, a flattened union annotated with
//      its origin should also display as `T | null`.
#[test]
fn union_of_lazy_alias_with_null_preserves_alias_name() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // type Foo = "a" | "b" | undefined
    let lit_a = db.literal_string("a");
    let lit_b = db.literal_string("b");
    let foo_body = db.union_literal_reduce(vec![lit_a, lit_b, TypeId::UNDEFINED]);
    let foo_name = db.intern_string("Foo");
    let foo_def = crate::def::DefinitionInfo::type_alias(foo_name, vec![], foo_body);
    let foo_def_id = def_store.register(foo_def);
    def_store.register_type_to_def(foo_body, foo_def_id);

    // Build the union from `[Lazy(Foo), null]`. Since Lazy is not a Union,
    // collect_union_members must NOT flatten it — the resulting union should
    // retain Lazy(Foo) as a top-level member.
    let foo_lazy = db.lazy(foo_def_id);
    let foo_or_null = db.union_literal_reduce(vec![foo_lazy, TypeId::NULL]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let rendered = fmt.format(foo_or_null);
    assert_eq!(rendered, "Foo | null", "got: {rendered}");
}

// Simulate the realistic case where the alias body has been substituted in
// place of Lazy(Foo) — i.e., the union members that reach the printer are
// the *flattened* union body plus `null`. Today we lose the alias name in
// this scenario; the new union-origin side table should restore it.
#[test]
fn union_with_origin_preserves_alias_name_after_flattening() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let lit_a = db.literal_string("a");
    let lit_b = db.literal_string("b");
    let foo_body = db.union_literal_reduce(vec![lit_a, lit_b, TypeId::UNDEFINED]);
    let foo_name = db.intern_string("Foo");
    let foo_def = crate::def::DefinitionInfo::type_alias(foo_name, vec![], foo_body);
    let foo_def_id = def_store.register(foo_def);
    def_store.register_type_to_def(foo_body, foo_def_id);

    // Pre-flattened union: [lit_a, lit_b, undefined, null]
    let flattened = db.union_literal_reduce(vec![lit_a, lit_b, TypeId::UNDEFINED, TypeId::NULL]);

    // Sanity: without origin, the printer must NOT know the alias.
    {
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let rendered = fmt.format(flattened);
        assert!(
            !rendered.contains("Foo"),
            "Pre-condition: structural form must not mention `Foo`. Got: {rendered}"
        );
    }

    // Record the as-written origin members [Lazy(Foo), null].
    let foo_lazy = db.lazy(foo_def_id);
    db.store_union_origin(flattened, vec![foo_lazy, TypeId::NULL]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let rendered = fmt.format(flattened);
    assert_eq!(rendered, "Foo | null", "got: {rendered}");
}

// Regression: tsc displays anonymous-object union members in declaration
// order, not in the canonical sort order our interner uses (by ShapeId).
// When source declares `var x: {} | { a: number };` after `{ a: number }`
// has already been interned (e.g., from an earlier `declare const`), the
// canonical sort puts `{ a: number; }` first because it has a smaller
// ShapeId. tsc would still show `{} | { a: number; }`. Storing the
// origin members lets the printer reproduce the source order.
//
// See: TypeScript/tests/cases/conformance/types/spread/spreadUnion2.ts
#[test]
fn store_union_origin_overrides_canonical_anon_object_sort() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Mimic the test fixture: `{ a: number }` is interned BEFORE `{}` so
    // the canonical sort would otherwise emit `{ a: number; } | {}`.
    let a_prop = PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER);
    let a_object = db.object(vec![a_prop]);
    let empty_object = db.object(vec![]);

    // Build the union as the user would have written it:
    // `{} | { a: number }`. The interner re-sorts these by ShapeId.
    // Use diagnostic mode to skip the synthetic `?: undefined`
    // optionalization (only relevant for hover/quickinfo, not errors).
    let union_id = crate::utils::union_or_single_literal_reduce(&db, vec![empty_object, a_object]);
    {
        let mut fmt = TypeFormatter::new(&db)
            .with_def_store(&def_store)
            .with_diagnostic_mode();
        let rendered = fmt.format(union_id);
        assert_eq!(
            rendered, "{ a: number; } | {}",
            "Pre-condition: canonical sort reorders by ShapeId"
        );
    }

    // Store the as-written origin members. Even though no flattening
    // occurred (2 in / 2 out), we should accept this because the canonical
    // order disagrees with the source order on anonymous Object members.
    db.store_union_origin(union_id, vec![empty_object, a_object]);

    let mut fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_diagnostic_mode();
    let rendered = fmt.format(union_id);
    assert_eq!(rendered, "{} | { a: number; }", "got: {rendered}");
}

// Locks the structural rule that the source-written order of an all-number-
// literal union survives the canonical sort, even when no flattening occurs
// and there are no anonymous object members. The canonical comparator only
// pins `0` first and falls back to allocation order for other number
// literals, so without origin storage the printer can render
// `0 | 1 | 2` as `0 | 2 | 1` (or any other alloc-order permutation) when
// the literals were interned in a different order earlier in the run.
//
// This regression mirrors `inDoesNotOperateOnPrimitiveTypes.ts` line 64,
// where tsc renders `T & (0 | 1 | 2)` but tsz had been rendering
// `T & (0 | 2 | 1)`.
#[test]
fn store_union_origin_preserves_source_order_for_number_literal_union() {
    let db = TypeInterner::new();

    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);
    let zero = db.literal_number(0.0);

    // Build the union in a source-written order that disagrees with TS7's
    // canonical numeric order: `2 | 1 | 0`.
    let origin = vec![two, one, zero];
    let union_id = db.union(origin.clone());

    // Pre-condition: without an origin, `compareTypes` sorts numeric literal
    // types by value, producing `0 | 1 | 2`.
    {
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(union_id), "0 | 1 | 2");
    }

    // Store the origin. Length is unchanged (3 in / 3 out) and there are no
    // anonymous object members, so the existing anon-object guard would
    // reject this. The number-literal guard must accept it.
    db.store_union_origin(union_id, origin);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union_id), "2 | 1 | 0");
}

#[test]
fn store_union_origin_preserves_source_order_for_type_parameter_union() {
    let db = TypeInterner::new();

    let top_name = db.intern_string("Top");
    let top = db.fresh_type_param(TypeParamInfo::simple(top_name));
    let u_name = db.intern_string("U");
    let u = db.fresh_type_param(TypeParamInfo::simple(u_name));
    let t_name = db.intern_string("T");
    let t = db.fresh_type_param(TypeParamInfo {
        name: t_name,
        constraint: Some(top),
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    });

    // This mirrors a two-pass type-parameter push: `U` keeps its first-pass
    // identity, while constrained `T` is allocated later. Canonical sorting
    // therefore prints `U | T`, even when the source union was `T | U`.
    let origin = vec![t, u];
    let union_id = db.union(origin.clone());
    {
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(union_id), "U | T");
    }

    db.store_union_origin(union_id, origin);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union_id), "T | U");
}

#[test]
fn store_union_origin_preserves_source_order_for_tuple_union() {
    let db = TypeInterner::new();

    // Force the canonical display order away from source order. #17383 made the
    // no-origin ordering follow the widened element rather than allocation
    // order, so `[] | [number, string]` is what the formatter renders for this
    // member set regardless of how the union was written. Declaring the source
    // as `[number, string] | []` therefore keeps the two orders distinct, which
    // is what makes the post-store assertion meaningful: if both renderings
    // agreed, this test would pass whether or not `store_union_origin` did
    // anything.
    let pair = db.tuple(vec![
        crate::types::TupleElement::fixed(TypeId::NUMBER),
        crate::types::TupleElement::fixed(TypeId::STRING),
    ]);
    let empty = db.tuple(vec![]);
    let origin = vec![pair, empty];
    let union_id = db.union(origin.clone());

    {
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(union_id), "[] | [number, string]");
    }

    db.store_union_origin(union_id, origin);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union_id), "[number, string] | []");
}

#[test]
fn formatter_can_ignore_union_origin_for_canonical_number_literal_display() {
    let db = TypeInterner::new();

    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);
    let zero = db.literal_number(0.0);
    // Source order `2 | 1 | 0` disagrees with TS7's canonical numeric order.
    let origin = vec![two, one, zero];
    let union_id = db.union(origin.clone());
    db.store_union_origin(union_id, origin);

    let mut source_order = TypeFormatter::new(&db);
    assert_eq!(source_order.format(union_id), "2 | 1 | 0");

    let mut canonical_order = TypeFormatter::new(&db).with_ignore_union_origins();
    assert_eq!(canonical_order.format(union_id), "0 | 1 | 2");
}

// Negative case: a number-literal-only union whose canonical order already
// matches the source order should NOT trigger origin storage. Storing it
// would waste memory and pin the order even if a later inferred-union
// caller passes a different (also-canonical) origin first.
#[test]
fn store_union_origin_skipped_when_number_literal_order_matches() {
    let db = TypeInterner::new();

    // Intern in order matching the canonical sort: 0 special-cased first,
    // then 1, then 2 by alloc order.
    let zero = db.literal_number(0.0);
    let one = db.literal_number(1.0);
    let two = db.literal_number(2.0);

    let union_id = db.union(vec![zero, one, two]);

    // Origin matches canonical — no need to override.
    db.store_union_origin(union_id, vec![zero, one, two]);

    assert!(
        db.get_union_origin(union_id).is_none(),
        "Origin must not be stored when canonical order already matches source"
    );
}

// Negative case: when the union members are non-anonymous (e.g., a literal
// and a Lazy alias), tsc and our interner agree on canonical sort. Storing
// the as-written origin in this case would override tsc's sort and regress
// diagnostics. The `<= origin_members.len()` guard must keep these out.
#[test]
fn store_union_origin_skips_canonical_sort_for_non_anon_members() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let foo_name = db.intern_string("Foo");
    let foo_def = crate::def::DefinitionInfo::type_alias(foo_name, vec![], TypeId::NUMBER);
    let foo_def_id = def_store.register(foo_def);
    def_store.register_type_to_def(TypeId::NUMBER, foo_def_id);
    let foo_lazy = db.lazy(foo_def_id);
    let lit_x = db.literal_string("x");

    // Build `Foo | "x"` — same length, no flattening, no anonymous object.
    let union_id = crate::utils::union_or_single_literal_reduce(&db, vec![foo_lazy, lit_x]);

    // Attempt to store an origin in REVERSED order. The guard should reject
    // this so the canonical structural form wins.
    db.store_union_origin(union_id, vec![lit_x, foo_lazy]);

    assert!(
        db.get_union_origin(union_id).is_none(),
        "Origin must be rejected when no anonymous object members are present"
    );
}

// Locks the structural rule that namespace and class-constructor definitions
// render as `typeof <Name>`, while interfaces and class-instance types render
// as bare `<Name>`. The picked conformance failure
// `jsElementAccessNoContextualTypeCrash` diverges from tsc by printing
// `Common` instead of `typeof Common`; the underlying invariant tested here
// is the correct branch in the formatter — fixing that test additionally
// requires the binder/checker to classify the `var Common = {}; Common.x =
// ...` JS-expando pattern as `DefKind::Namespace`.
#[test]
fn typeof_prefix_for_namespace_and_class_constructor_defs() {
    fn make_def(
        db: &TypeInterner,
        kind: crate::def::DefKind,
        name: &str,
    ) -> crate::def::DefinitionInfo {
        crate::def::DefinitionInfo {
            kind,
            name: db.intern_string(name),
            type_params: Vec::new(),
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
        }
    }

    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Each backing type gets one distinct property so its TypeId is not
    // interned to the universally-shared empty object `{}` (which the
    // formatter intentionally never repaints with an alias name).
    let ns_obj = db.object(vec![PropertyInfo::new(
        db.intern_string("ns_marker"),
        TypeId::STRING,
    )]);
    let class_ctor_obj = db.object(vec![PropertyInfo::new(
        db.intern_string("class_ctor_marker"),
        TypeId::STRING,
    )]);
    let iface_obj = db.object(vec![PropertyInfo::new(
        db.intern_string("iface_marker"),
        TypeId::STRING,
    )]);
    let class_instance_obj = db.object(vec![PropertyInfo::new(
        db.intern_string("class_instance_marker"),
        TypeId::STRING,
    )]);

    let ns_def_id = def_store.register(make_def(&db, crate::def::DefKind::Namespace, "Common"));
    def_store.register_type_to_def(ns_obj, ns_def_id);

    let class_ctor_def_id =
        def_store.register(make_def(&db, crate::def::DefKind::ClassConstructor, "Foo"));
    def_store.register_type_to_def(class_ctor_obj, class_ctor_def_id);

    let iface_def_id = def_store.register(make_def(&db, crate::def::DefKind::Interface, "IFoo"));
    def_store.register_type_to_def(iface_obj, iface_def_id);

    let class_def_id = def_store.register(make_def(&db, crate::def::DefKind::Class, "Bar"));
    def_store.register_type_to_def(class_instance_obj, class_def_id);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(ns_obj), "typeof Common");
    assert_eq!(fmt.format(class_ctor_obj), "typeof Foo");
    assert_eq!(fmt.format(iface_obj), "IFoo");
    assert_eq!(fmt.format(class_instance_obj), "Bar");
}

/// `T[]` is a `TypeReference` to the global `Array` in tsc, so under TS7's
/// `stableTypeOrdering` it sorts under the name "Array" via `compareTypeNames`
/// (`checker.ts:53867`) -- ahead of any named member sorting after "Array",
/// regardless of the order the union was written in.
///
/// Verified against tsc 7.0.2:
/// ```text
/// interface Cover { color: string; }
/// declare function f(c: Cover | Cover[]): void;
/// f({ color: "red", couleur: "rouge" });
/// // error TS2353: ... does not exist in type 'Cover[] | Cover'.
/// ```
/// Note the source is written `Cover | Cover[]` and tsc still renders
/// `Cover[] | Cover`. The conformance oracle agrees
/// (`compiler/objectLiteralExcessProperties.ts`).
///
/// The element-source-position inheritance this test originally pinned is still
/// the tie-break for arrays whose names compare equal; it is simply no longer
/// the deciding key when the names differ.
#[test]
fn union_array_sorts_under_the_array_name() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let cover = crate::def::DefinitionInfo::interface(
        db.intern_string("Cover"),
        vec![],
        vec![PropertyInfo::new(db.intern_string("color"), TypeId::STRING)],
    )
    .with_file_id(0)
    .with_span(100, 110);
    let cover_def = def_store.register(cover);
    let cover_ref = db.lazy(cover_def);
    let cover_array = db.array(cover_ref);

    // Source order: `Cover | Cover[]`.
    let union_id = db.union_preserve_members(vec![cover_ref, cover_array]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(union_id),
        "Cover[] | Cover",
        "`Cover[]` sorts under the global name \"Array\", which precedes \"Cover\""
    );
}

#[test]
fn union_array_of_intrinsic_uses_typescript_7_flag_order() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let react_child = crate::def::DefinitionInfo::type_alias(
        db.intern_string("ReactChild"),
        vec![],
        TypeId::STRING,
    )
    .with_file_id(0)
    .with_span(100, 110);
    let react_child_def = def_store.register(react_child);
    let react_child_ref = db.lazy(react_child_def);
    let any_array = db.array(TypeId::ANY);

    let union_id = db.union_preserve_members(vec![react_child_ref, any_array, TypeId::BOOLEAN]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    // `ReactChild = string` is a primitive-bodied alias, so it renders as
    // `string` (tsc attaches no `aliasSymbol` to the shared intrinsic). The
    // point of this test is TypeScript 7 stable ordering: `string` and
    // `boolean` use their primitive TypeFlags positions, while `any[]` is an
    // object and follows both.
    assert_eq!(
        fmt.format(union_id),
        "string | boolean | any[]",
        "Arrays of intrinsic element types should use their object-type ordering"
    );
}

/// TypeScript 7 compares visible generic base names before source positions.
#[test]
fn union_application_uses_typescript_7_visible_name_order() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // A built-in-like generic with a low source position.
    let container = crate::def::DefinitionInfo::interface(
        db.intern_string("Container"),
        vec![TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        }],
        vec![],
    )
    .with_file_id(0)
    .with_span(0, 10);
    let container_def = def_store.register(container);

    // A user type with a much later source position.
    let user_iface = crate::def::DefinitionInfo::interface(
        db.intern_string("Item"),
        vec![],
        vec![PropertyInfo::new(db.intern_string("v"), TypeId::STRING)],
    )
    .with_file_id(0)
    .with_span(500, 510);
    let user_def = def_store.register(user_iface);
    let user_ref = db.lazy(user_def);

    let application = db.application(db.lazy(container_def), vec![user_ref]);

    // Source order: `Item | Container<Item>`.
    let union_id = db.union_preserve_members(vec![user_ref, application]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(union_id);
    assert_eq!(result, "Container<Item> | Item");
}

/// Regression: a union mixing a named type with a string literal renders the
/// literal first under TypeScript 7's `TypeFlags` order — string literals rank
/// (`1 << 10`) below object-like named types (`1 << 20`). Oracle-verified
/// against typescript@7.0.2, which prints `"foo" | Refrigerator`.
///
/// Source: `stringLiteralsWithEqualityChecks03` (and 04).
#[test]
fn union_string_literal_renders_before_named_type() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Build an interface with a real source position so it lands in tier 1.
    let iface = crate::def::DefinitionInfo::interface(
        db.intern_string("Refrigerator"),
        vec![],
        vec![PropertyInfo::new(
            db.intern_string("makesFoodGoBrrr"),
            TypeId::BOOLEAN,
        )],
    )
    .with_file_id(0)
    .with_span(10, 20);
    let iface_def_id = def_store.register(iface);
    let iface_ref = db.lazy(iface_def_id);

    // Insertion order matches the source `let y: "foo" | Refrigerator`.
    let union_id = db.union_preserve_members(vec![db.literal_string("foo"), iface_ref]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(union_id);
    assert_eq!(
        result, "\"foo\" | Refrigerator",
        "TypeScript 7 ranks string literals below object-like named types"
    );
}

/// Sibling test: string literals (rank `1 << 10`) render before object-like
/// named types (rank `1 << 20`); within each rank, literals sort by value and
/// named types alphabetically. Oracle-verified against typescript@7.0.2, which
/// prints `"x" | "y" | Alpha | Beta`.
#[test]
fn union_literals_render_before_sorted_named_types() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let alpha = crate::def::DefinitionInfo::interface(db.intern_string("Alpha"), vec![], vec![])
        .with_file_id(0)
        .with_span(10, 20);
    let beta = crate::def::DefinitionInfo::interface(db.intern_string("Beta"), vec![], vec![])
        .with_file_id(0)
        .with_span(30, 40);
    let alpha_def = def_store.register(alpha);
    let beta_def = def_store.register(beta);
    let alpha_ref = db.lazy(alpha_def);
    let beta_ref = db.lazy(beta_def);

    // Source order: `"x" | Beta | "y" | Alpha`.
    let union_id = db.union_preserve_members(vec![
        db.literal_string("x"),
        beta_ref,
        db.literal_string("y"),
        alpha_ref,
    ]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(union_id);
    // Literals rank below named types, sorted by value ("x" < "y"); the named
    // interfaces follow, alphabetized (Alpha < Beta).
    assert_eq!(result, "\"x\" | \"y\" | Alpha | Beta");
}

/// tsc renders the eight `typeof` result string literals alphabetically
/// (`"bigint" | "boolean" | "function" | ...`) through the shared union
/// comparator — string literals rank together, sorted by value — which is
/// the exact order the pinned tsc 7.0.2 emits in TS2367/TS2322 diagnostics.
/// The interner's allocation history (which can put `"symbol"` ahead of
/// `"string"` after lib processing) must not leak through.
#[test]
fn typeof_result_union_renders_alphabetically() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    // Build the union with `"symbol"` first so the interner's allocation
    // order can't satisfy the expected output by accident.
    let members = vec![
        db.literal_string("symbol"),
        db.literal_string("function"),
        db.literal_string("object"),
        db.literal_string("undefined"),
        db.literal_string("boolean"),
        db.literal_string("bigint"),
        db.literal_string("number"),
        db.literal_string("string"),
    ];
    let union_id = db.union_preserve_members(members);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(union_id);
    assert_eq!(
        result,
        "\"bigint\" | \"boolean\" | \"function\" | \"number\" | \"object\" | \"string\" | \"symbol\" | \"undefined\""
    );
}

/// A union whose `union_origin` preserves a nested *anonymous* sub-union
/// (e.g. the `number | undefined` produced by `T[K]` inside a homomorphic
/// mapped template `{ [K in keyof T]: T[K] | null }`) must still render
/// `null`/`undefined` at the canonical tail — `number | null | undefined`,
/// matching tsc — not leak the nested `undefined` ahead of `null`.
#[test]
fn nested_anonymous_union_origin_hoists_nullish_to_tail() {
    let db = TypeInterner::new();
    let inner = db.union(vec![TypeId::NUMBER, TypeId::UNDEFINED]); // number | undefined
    let outer = db.union(vec![TypeId::NUMBER, TypeId::UNDEFINED, TypeId::NULL]);
    // Simulate the `T[K] | null` origin preserving the nested sub-union.
    db.store_union_origin(outer, vec![inner, TypeId::NULL]);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(outer), "number | null | undefined");
}

/// The hoist must keep the non-nullish remainder of a nested anonymous union
/// cohesive: `(string | number | undefined) | null` renders as
/// `string | number | null | undefined`.
#[test]
fn nested_anonymous_union_origin_keeps_remainder_cohesive() {
    let db = TypeInterner::new();
    let inner = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::UNDEFINED]);
    let outer = db.union(vec![
        TypeId::STRING,
        TypeId::NUMBER,
        TypeId::UNDEFINED,
        TypeId::NULL,
    ]);
    db.store_union_origin(outer, vec![inner, TypeId::NULL]);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(outer), "string | number | null | undefined");
}

// =================================================================
// Cyclic source-position graph guard (issue #7574 — 4th overflow).
//
// `get_source_position_for_type` walks `Application` bases/args and
// display-alias origins to compute union display order. A self-referential
// generic instantiation (e.g. cross-file `Foo<Bar<Foo<...>>>` chains in
// large-ts-repo's infrastructure glob) makes that walk cyclic, which used to
// overflow the native stack via the mutual recursion with
// `get_application_source_position`. A per-call visited set now terminates the
// walk by returning the tier-2 "no source info" sentinel on re-entry.
// =================================================================

/// Build a 2-cycle through the display-alias + Application-args path and assert
/// the source-position walk terminates instead of overflowing the stack.
fn assert_source_position_cycle_terminates(arg_first: bool) {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // Two distinct evaluated object types so each can carry its own display
    // alias (store_display_alias ignores identical evaluated==application).
    let eval_a = db.object(vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)]);
    let eval_b = db.object(vec![PropertyInfo::new(db.intern_string("b"), TypeId::STRING)]);

    // Applications that reference the *other* evaluated type, forming a cycle:
    //   eval_a --alias--> App(base, [eval_b]) --arg--> eval_b
    //   eval_b --alias--> App(base, [eval_a]) --arg--> eval_a
    // Put the cross-reference in the arg or the base position depending on the
    // matrix variant so both recursion edges (line 547 base, line 552 args) are
    // covered.
    let base = db.lazy(crate::def::DefId(7001));
    let (app_a, app_b) = if arg_first {
        (
            db.application(base, vec![eval_b]),
            db.application(base, vec![eval_a]),
        )
    } else {
        (db.application(eval_b, vec![]), db.application(eval_a, vec![]))
    };
    db.store_display_alias(eval_a, app_a);
    db.store_display_alias(eval_b, app_b);

    let fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    // The guard must make this return rather than overflow the stack.
    let _ = fmt.get_source_position_for_type(eval_a, &def_store);
    let _ = fmt.get_source_position_for_type(eval_b, &def_store);
}

#[test]
fn source_position_cycle_via_application_args_terminates() {
    assert_source_position_cycle_terminates(true);
}

#[test]
fn source_position_cycle_via_application_base_terminates() {
    assert_source_position_cycle_terminates(false);
}

/// Deep-but-finite chain of nested applications must NOT be short-circuited by
/// the guard: it walks to the bottom and returns the user-defined element's
/// tier, proving the guard fires only on genuine cycles, never on legitimate
/// finite depth.
#[test]
fn source_position_deep_finite_chain_is_not_short_circuited() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let base = db.lazy(crate::def::DefId(7100));

    // Build App(base, [App(base, [ ... [leaf] ... ])]) 200 levels deep over a
    // distinct, non-cyclic chain (each application is a fresh content shape).
    let leaf = db.object(vec![PropertyInfo::new(db.intern_string("leaf"), TypeId::NUMBER)]);
    let mut current = leaf;
    for depth in 0..200u32 {
        // Vary the base so each nesting level is a distinct interned application
        // and the chain stays acyclic.
        let level_base = db.lazy(crate::def::DefId(7200 + depth));
        current = db.application(level_base, vec![current, base]);
    }

    let fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let (tier, _file, _span) = fmt.get_source_position_for_type(current, &def_store);
    // A finite chain over a user-defined leaf object resolves to a concrete
    // tier (1 or 2), and crucially the call returns at all.
    assert!(tier <= 2, "finite application chain must resolve to a valid tier");
}

// =================================================================
// Anonymous composite structural display (#14827)
// =================================================================
//
// Structural interning collapses an inline annotation (`{ a: number }`) and a
// coincidentally-shaped non-generic type-alias body (`type A = { a: number }`)
// onto one `TypeId`, so the reverse `find_def_for_type` / `find_def_by_shape`
// lookup cannot prove the source referenced the alias. tsc spells the alias
// name only when the reference carried an `aliasSymbol`; an inline annotation
// carries none and is rendered structurally. The `with_anonymous_composite_structural`
// mode lets callers that know the operand came from an inline composite
// annotation suppress the unsound alias-name repaint.
#[test]
fn anonymous_composite_structural_object_alias_renders_structurally() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.object(vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)]);
    let def = crate::def::DefinitionInfo::type_alias(db.intern_string("A"), vec![], body);
    let def_id = def_store.register(def);
    def_store.register_type_to_def(body, def_id);

    // Default: a direct alias reference keeps the alias name.
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(body), "A");

    // Anonymous-composite mode: inline annotation renders structurally.
    let mut fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_anonymous_composite_structural();
    assert_eq!(fmt.format(body), "{ a: number; }");
}

#[test]
fn anonymous_composite_structural_union_members_render_structurally() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let a_body = db.object(vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)]);
    let a_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("A"),
        vec![],
        a_body,
    ));
    def_store.register_type_to_def(a_body, a_def);

    let b_body = db.object(vec![PropertyInfo::new(db.intern_string("b"), TypeId::STRING)]);
    let b_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("B"),
        vec![],
        b_body,
    ));
    def_store.register_type_to_def(b_body, b_def);

    // Format the alias bodies through a union node. The anonymous-composite
    // mode must render every member's structural shape and never repaint a
    // member with its coincidental non-generic alias name (`A` / `B`).
    let union = db.union(vec![a_body, b_body]);
    let mut fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_anonymous_composite_structural();
    let rendered = fmt.format(union);
    assert!(
        rendered.contains("a: number") && rendered.contains("b: string"),
        "members should render structurally; got: {rendered}"
    );
    assert!(
        !rendered
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|tok| tok == "A" || tok == "B"),
        "no member should be repainted with its alias name; got: {rendered}"
    );
}

#[test]
fn anonymous_composite_structural_preserves_interface_name() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // A nominal interface is registered against the same structural shape. The
    // anonymous-composite mode must NOT strip its name: nominal types are not
    // subject to the alias-provenance unsoundness.
    let body = db.object(vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)]);
    let def = crate::def::DefinitionInfo::interface(
        db.intern_string("Named"),
        vec![],
        vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)],
    );
    let def_id = def_store.register(def);
    def_store.register_type_to_def(body, def_id);

    let mut fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_anonymous_composite_structural();
    assert_eq!(fmt.format(body), "Named");
}
