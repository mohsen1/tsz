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
    };
    let f_param = TypeParamInfo {
        name: db.intern_string("F"),
        constraint: None,
        default: None,
        is_const: false,
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
        single_quoted_name: false,
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
        single_quoted_name: false,
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
        single_quoted_name: false,
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
fn distributive_conditional_alias_with_non_boolean_singleton_keeps_alias() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let foo_lazy = build_distributive_foo_alias(&db, &def_store);

    // Application(Foo, [string]) — singleton arg; no distribution.
    let app = db.application(foo_lazy, vec![TypeId::STRING]);
    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(app);

    // No distribution because `string` is neither `boolean` nor a Union.
    // The formatter should preserve the alias-application form.
    assert_eq!(
        result, "Foo<string>",
        "Singleton non-distributable args must keep the alias name. Got: {result}"
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

    // Force a non-source allocation order: intern `2` before `1` so the
    // canonical sort's alloc-order fallback puts `2` ahead of `1`.
    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);
    let zero = db.literal_number(0.0);

    // Build the union in source-written order: `0 | 1 | 2`.
    let origin = vec![zero, one, two];
    let union_id = db.union(origin.clone());

    // Pre-condition: without an origin, the canonical sort produces
    // `0 | 2 | 1` because alloc_order(2) < alloc_order(1).
    {
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(union_id), "0 | 2 | 1");
    }

    // Store the origin. Length is unchanged (3 in / 3 out) and there are no
    // anonymous object members, so the existing anon-object guard would
    // reject this. The number-literal guard must accept it.
    db.store_union_origin(union_id, origin);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(union_id), "0 | 1 | 2");
}

#[test]
fn formatter_can_ignore_union_origin_for_canonical_number_literal_display() {
    let db = TypeInterner::new();

    let two = db.literal_number(2.0);
    let one = db.literal_number(1.0);
    let zero = db.literal_number(0.0);
    let origin = vec![zero, one, two];
    let union_id = db.union(origin.clone());
    db.store_union_origin(union_id, origin);

    let mut source_order = TypeFormatter::new(&db);
    assert_eq!(source_order.format(union_id), "0 | 1 | 2");

    let mut canonical_order = TypeFormatter::new(&db).with_ignore_union_origins();
    assert_eq!(canonical_order.format(union_id), "0 | 2 | 1");
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

/// Regression: `T[]` (modeled as `TypeData::Array(T)`) should inherit its
/// element type's source position when sorting union members. Without this,
/// `Cover[]` falls through to the tier-2 sentinel and a union written as
/// `Cover | Cover[]` displays out of order.
#[test]
fn union_array_inherits_element_source_position() {
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
        "Cover | Cover[]",
        "Array(T) should inherit T's position so `Cover | Cover[]` stays in source order"
    );
}

#[test]
fn union_array_of_intrinsic_stays_after_primitive_builtin() {
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
    // point of this test is the union member ordering: the array of an
    // intrinsic element type must not inherit `any`'s low builtin key, so it
    // stays after the `boolean` primitive rather than sorting to the front.
    assert_eq!(
        fmt.format(union_id),
        "boolean | any[] | string",
        "Arrays of intrinsic element types should not inherit `any`'s low builtin key"
    );
}

/// Regression: `Application(Container, [T])` should use the MAX position of
/// the base and its arguments. This keeps generic instantiations sorted with
/// the user-defined element type rather than with a built-in / lib base.
#[test]
fn union_application_uses_max_arg_position() {
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
    // `Container<Item>` inherits Item's position via the MAX rule, so the
    // union preserves source order.
    assert_eq!(result, "Item | Container<Item>");
}

/// Regression: a union mixing a named type (tier 1, has source position) with
/// a literal type (tier 2, no source position) should display the named type
/// first, matching tsc. Source order alone — `"foo" | Refrigerator` — is not
/// what tsc renders; tsc displays `Refrigerator | "foo"`.
///
/// Source: `stringLiteralsWithEqualityChecks03` (and 04).
#[test]
fn union_named_type_renders_before_string_literal() {
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
        result, "Refrigerator | \"foo\"",
        "Named type (tier 1) must render before a literal (tier 2) regardless of source order"
    );
}

/// Sibling test: multiple named types stay sorted by source position, and any
/// number of trailing literals retain their relative declaration order.
#[test]
fn union_multiple_named_types_sorted_then_literals_in_source_order() {
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
    // Named types come first, sorted by span (Alpha at 10 < Beta at 30).
    // Literals follow in the order they appeared in the input.
    assert_eq!(result, "Alpha | Beta | \"x\" | \"y\"");
}

/// Regression: tsc renders the eight `typeof` result string literals in
/// JS-spec order regardless of how the interner pre-sorted them. The
/// interner can put `"symbol"` ahead of `"string"` whenever lib processing
/// of `Symbol.toPrimitive` allocates the literal first; without the
/// canonical-order carve-out, that allocation history leaks into TS2367
/// overlap diagnostics.
#[test]
fn typeof_result_union_renders_in_canonical_order() {
    let db = TypeInterner::new();
    // Build the union with `"symbol"` first so the interner's
    // allocation order (and any input-order-based sort) can't satisfy the
    // expected output by accident.
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

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    assert_eq!(
        result,
        "\"string\" | \"number\" | \"bigint\" | \"boolean\" | \"symbol\" | \"undefined\" | \"object\" | \"function\""
    );
}

/// Anti-regression: a SUBSET of the typeof literals must NOT be reordered
/// — only the exact eight-member set is the JS-spec `typeof` vocabulary,
/// and reordering arbitrary string-literal subsets would break legitimate
/// declaration-order display elsewhere.
#[test]
fn typeof_result_carve_out_does_not_apply_to_subset() {
    let db = TypeInterner::new();
    let members = vec![
        db.literal_string("symbol"),
        db.literal_string("string"),
        db.literal_string("number"),
    ];
    let union_id = db.union_preserve_members(members);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(union_id);
    // Three-element subset must keep the input order — not reordered to
    // tsc's typeof canonical order (which would put "string" first).
    assert!(
        !result.starts_with("\"string\""),
        "Three-literal subset `symbol | string | number` must NOT be reordered to tsc's typeof canonical order; got: {result}"
    );
}
