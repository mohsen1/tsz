#[test]
fn format_tuple_named_optional_element() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![crate::types::TupleElement {
        type_id: TypeId::STRING,
        name: Some(db.intern_string("name")),
        optional: true,
        rest: false,
    }]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[name?: string | undefined]");
}

#[test]
fn format_tuple_optional_elements_preserve_surface_in_exact_optional_mode() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db).with_exact_optional_property_types(true);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: Some(db.intern_string("count")),
            optional: true,
            rest: false,
        },
    ]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[string?, count?: number]");
}

#[test]
fn format_tuple_optional_absorbing_types_keep_suffix_form() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::ANY,
            name: None,
            optional: true,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::UNKNOWN,
            name: Some(db.intern_string("value")),
            optional: true,
            rest: false,
        },
    ]);
    let result = fmt.format(tuple);
    assert_eq!(result, "[any?, value?: unknown]");
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

// =================================================================
// Mapped type formatting
// =================================================================

#[test]
fn format_mapped_type_basic() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let result = fmt.format(mapped);
    assert_eq!(result, "{ [x: string]: number; }");
}

#[test]
fn format_mapped_type_with_remove_optional() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: Some(MappedModifier::Remove),
    });
    let result = fmt.format(mapped);
    assert!(
        result.contains("]-?:"),
        "Expected remove optional modifier '-?', got: {result}"
    );
}

#[test]
fn format_mapped_type_with_remove_readonly() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("K"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: Some(MappedModifier::Remove),
        optional_modifier: None,
    });
    let result = fmt.format(mapped);
    assert!(
        result.contains("-readonly"),
        "Expected remove readonly modifier, got: {result}"
    );
}

#[test]
fn format_mapped_string_index_signature_like() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: TypeId::NUMBER,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert_eq!(fmt.format(mapped), "{ [x: string]: number; }");
}

#[test]
fn format_mapped_preserves_key_dependent_template() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    let key_name = db.intern_string("P");
    let key_param = db.type_param(TypeParamInfo {
        name: key_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: TypeId::STRING,
        template: key_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    assert_eq!(fmt.format(mapped), "{ [P in string]: P; }");
}

// =================================================================
// Template literal formatting
// =================================================================

#[test]
fn format_template_literal_text_only() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![TemplateSpan::Text(db.intern_string("hello"))]);
    // Text-only template literals may be simplified by the interner
    // but if they survive, they should format with backticks
    let result = fmt.format(tl);
    assert!(
        result.contains("hello"),
        "Expected 'hello' in template literal, got: {result}"
    );
}

#[test]
fn format_template_literal_with_type_interpolation() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![
        TemplateSpan::Text(db.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let result = fmt.format(tl);
    assert!(
        result.contains("hello "),
        "Expected 'hello ' prefix, got: {result}"
    );
    assert!(
        result.contains("${string}"),
        "Expected '${{string}}' interpolation, got: {result}"
    );
}

#[test]
fn format_template_literal_complex() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tl = db.template_literal(vec![
        TemplateSpan::Text(db.intern_string("key_")),
        TemplateSpan::Type(TypeId::NUMBER),
        TemplateSpan::Text(db.intern_string("_suffix")),
    ]);
    let result = fmt.format(tl);
    assert!(result.contains("key_"), "Expected 'key_', got: {result}");
    assert!(
        result.contains("${number}"),
        "Expected '${{number}}', got: {result}"
    );
    assert!(
        result.contains("_suffix"),
        "Expected '_suffix', got: {result}"
    );
}

#[test]
fn format_template_literal_flattens_nested_alias_interpolations() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let number_pattern = db.template_literal(vec![TemplateSpan::Type(TypeId::NUMBER)]);
    let alias_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("A"),
        vec![],
        number_pattern,
    ));
    let alias_ref = db.lazy(alias_def);
    let spaced = db.template_literal(vec![
        TemplateSpan::Type(alias_ref),
        TemplateSpan::Text(db.intern_string(" ")),
        TemplateSpan::Type(alias_ref),
    ]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(spaced), "`${number} ${number}`");
}

#[test]
fn format_template_literal_pattern_union_does_not_repaint_application_alias() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let protocol_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Protocol"),
        vec![
            TypeParamInfo {
                name: db.intern_string("T"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("U"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            },
        ],
        TypeId::STRING,
    ));
    let protocols = db.union(vec![
        db.literal_string("http"),
        db.literal_string("https"),
        db.literal_string("ftp"),
    ]);
    let evaluated = db.template_literal(vec![
        TemplateSpan::Type(protocols),
        TemplateSpan::Text(db.intern_string("://")),
        TemplateSpan::Type(TypeId::STRING),
    ]);
    let app = db.application(db.lazy(protocol_def), vec![protocols, TypeId::STRING]);
    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(evaluated);

    assert!(
        !result.starts_with("Protocol<"),
        "template pattern unions should show the expanded pattern, got {result}"
    );
    assert!(result.contains("`http://${string}`"), "{result}");
    assert!(result.contains("`https://${string}`"), "{result}");
    assert!(result.contains("`ftp://${string}`"), "{result}");
}

// =================================================================
// String intrinsic formatting
// =================================================================

#[test]
fn format_string_intrinsic_uppercase() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let upper = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
    assert_eq!(fmt.format(upper), "Uppercase<string>");
}

#[test]
fn format_string_intrinsic_lowercase() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let lower = db.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);
    assert_eq!(fmt.format(lower), "Lowercase<string>");
}

#[test]
fn format_string_intrinsic_capitalize() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let cap = db.string_intrinsic(StringIntrinsicKind::Capitalize, TypeId::STRING);
    assert_eq!(fmt.format(cap), "Capitalize<string>");
}

#[test]
fn format_string_intrinsic_uncapitalize() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let uncap = db.string_intrinsic(StringIntrinsicKind::Uncapitalize, TypeId::STRING);
    assert_eq!(fmt.format(uncap), "Uncapitalize<string>");
}

// =================================================================
// Error type formatting
// =================================================================

#[test]
fn format_error_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(TypeId::ERROR), "error");
}

// =================================================================
// Depth limiting (deeply nested types)
// =================================================================

#[test]
fn format_deeply_nested_array_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create deeply nested arrays: string[][][][][][]...
    let mut current = TypeId::STRING;
    for _ in 0..10 {
        current = db.array(current);
    }
    let result = fmt.format(current);
    // At some depth, the formatter should produce "..." due to max_depth
    assert!(
        result.contains("..."),
        "Deeply nested type should hit depth limit and show '...', got: {result}"
    );
}

#[test]
fn format_deeply_nested_union_truncated() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create nested unions: wrap in array at each level to increase depth
    let mut current = TypeId::STRING;
    for _ in 0..10 {
        let inner_union = db.union2(current, TypeId::NUMBER);
        current = db.array(inner_union);
    }
    let result = fmt.format(current);
    // Should hit depth limit
    assert!(
        result.contains("..."),
        "Deeply nested type should truncate, got: {result}"
    );
}

#[test]
fn format_deeply_nested_object_uses_object_elision() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let mut current = db.object(vec![PropertyInfo::new(
        db.intern_string("leaf"),
        TypeId::NUMBER,
    )]);
    for i in 0..12 {
        current = db.object(vec![PropertyInfo::new(
            db.intern_string(&format!("p{i}")),
            current,
        )]);
    }

    let result = fmt.format(current);
    assert!(
        result.contains("{ ...; }"),
        "Deeply nested object branches should elide as '{{ ...; }}', got: {result}"
    );
}

// =================================================================
// Special types
// =================================================================

#[test]
fn format_type_parameter() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let tp = db.type_param(TypeParamInfo {
        name: db.intern_string("MyType"),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert_eq!(fmt.format(tp), "MyType");
}

#[test]
fn format_keyof_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let keyof = db.keyof(TypeId::STRING);
    assert_eq!(fmt.format(keyof), "keyof string");
}

#[test]
fn format_keyof_intersection_distributes() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let intersection = db.intersection2(t, u);
    let keyof = db.keyof(intersection);

    assert_eq!(fmt.format(keyof), "keyof T | keyof U");
}

#[test]
fn format_keyof_nullish_collapses_to_never() {
    // tsc reduces `keyof null`, `keyof undefined`, and `keyof void`
    // to `never` in error messages. The evaluator already maps these to
    // TypeId::NEVER; the formatter must not bypass that reduction.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    assert_eq!(fmt.format(db.keyof(TypeId::NULL)), "never");
    assert_eq!(fmt.format(db.keyof(TypeId::UNDEFINED)), "never");
    assert_eq!(fmt.format(db.keyof(TypeId::VOID)), "never");
    assert_eq!(
        fmt.format(db.keyof(TypeId::NEVER)),
        "string | number | symbol"
    );
}

#[test]
fn format_keyof_intersection_with_empty_object_does_not_distribute() {
    // tsc preserves `keyof (T & {})` undistributed in error messages.
    // Distributing to `keyof T | keyof {}` is technically equivalent but
    // breaks fingerprint parity (e.g. unknownControlFlow.ts ff3).
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let t = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let empty_obj = db.object(vec![]);
    let intersection = db.intersection2(t, empty_obj);
    let keyof = db.keyof(intersection);

    assert_eq!(fmt.format(keyof), "keyof (T & {})");
}

#[test]
fn format_readonly_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let ro = db.readonly_type(TypeId::NUMBER);
    assert_eq!(fmt.format(ro), "readonly number");
}

#[test]
fn format_index_access_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let idx = db.index_access(TypeId::STRING, TypeId::NUMBER);
    assert_eq!(fmt.format(idx), "string[number]");
}

/// Helper for the homomorphic-mapped indexed-access tests below: builds a
/// homomorphic identity Mapped type `{ readonly? [bound in keyof source]?: source[bound] }`
/// using `bound_name` as the iteration variable name. Switching `bound_name`
/// across tests guards against any name-hardcoded simplification logic.
fn make_homomorphic_mapped(
    db: &TypeInterner,
    source: TypeId,
    bound_name: &str,
    optional: Option<MappedModifier>,
    readonly: Option<MappedModifier>,
) -> TypeId {
    let bound = db.type_param(TypeParamInfo {
        name: db.intern_string(bound_name),
        constraint: None,
        default: None,
        is_const: false,
    });
    let template = db.index_access(source, bound);
    db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string(bound_name),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: db.keyof(source),
        template,
        name_type: None,
        readonly_modifier: readonly,
        optional_modifier: optional,
    })
}

#[test]
fn homomorphic_mapped_index_access_partial_simplifies() {
    // `Partial<U>[K]` — homomorphic identity Mapped with `optional_modifier = Add` —
    // displays as `U[K] | undefined`, matching tsc, instead of the structural
    // `{ [P in keyof U]?: U[P] | undefined; }[K]` form.
    let db = TypeInterner::new();
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let partial_u = make_homomorphic_mapped(&db, u, "P", Some(MappedModifier::Add), None);
    let access = db.index_access(partial_u, k);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(access), "U[K] | undefined");
}

#[test]
fn homomorphic_mapped_index_access_readonly_simplifies() {
    // `Readonly<U>[K]` — homomorphic identity Mapped with `readonly_modifier = Add` —
    // displays as `U[K]` (readonly is a property-level modifier and does not
    // appear in indexed-access value types).
    let db = TypeInterner::new();
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let readonly_u = make_homomorphic_mapped(&db, u, "P", None, Some(MappedModifier::Add));
    let access = db.index_access(readonly_u, k);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(fmt.format(access), "U[K]");
}

#[test]
fn homomorphic_mapped_index_access_independent_of_bound_name() {
    // The simplification is structural: it must hold for any iteration
    // variable name (P, X, Q, ...). A name-hardcoded check (e.g. `name == "P"`)
    // would silently fall back to the structural form for non-`P` aliases.
    let db = TypeInterner::new();
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let key_index = db.keyof(u);
    let partial_u_q = make_homomorphic_mapped(&db, u, "Q", Some(MappedModifier::Add), None);
    let partial_u_x = make_homomorphic_mapped(&db, u, "X", Some(MappedModifier::Add), None);

    let mut fmt = TypeFormatter::new(&db);
    assert_eq!(
        fmt.format(db.index_access(partial_u_q, key_index)),
        "U[keyof U] | undefined"
    );
    assert_eq!(
        fmt.format(db.index_access(partial_u_x, key_index)),
        "U[keyof U] | undefined"
    );
}

#[test]
fn homomorphic_mapped_index_access_skips_non_identity_template() {
    // A non-homomorphic mapped (template body is not `source[P]`) must keep
    // the structural display, since `M[K]` no longer reduces to `source[K]`.
    let db = TypeInterner::new();
    let u = db.type_param(TypeParamInfo {
        name: db.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo {
            name: db.intern_string("P"),
            constraint: None,
            default: None,
            is_const: false,
        },
        constraint: db.keyof(u),
        template: TypeId::NUMBER, // not `U[P]`
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });
    let access = db.index_access(mapped, k);

    let mut fmt = TypeFormatter::new(&db);
    let formatted = fmt.format(access);
    assert!(
        formatted.contains("[P in keyof U]"),
        "expected structural mapped form for non-homomorphic template, got: {formatted}"
    );
}

/// When a generic type alias evaluates to a fresh remapped Mapped type, the
/// display alias must be stored so that `Alias<K>[IndexType]` shows the alias
/// name instead of the expanded `{ [P in K as ...]: ... }[IndexType]` form.
///
/// Structural rule: for any `Application(Alias, [T])` where `T` contains a
/// generic type param and the body evaluates to a fresh Mapped node, the
/// `IndexAccess` display must use the alias name.
#[test]
fn remapped_mapped_type_alias_index_access_shows_alias_name() {
    // Simulates: `type GetterMap<K extends string> = { [P in K as `get${P}`]: { a: P } }`
    // and an IndexAccess: `GetterMap<K>['get${K}']`
    let db = TypeInterner::new();

    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let p = db.type_param(TypeParamInfo::simple(db.intern_string("P")));
    // Template: { a: P }
    let a_prop = db.intern_string("a");
    let template = db.object(vec![PropertyInfo::new(a_prop, p)]);
    // name_type: `get${P}`
    let get_prefix = db.intern_string("get");
    let name_type =
        db.template_literal(vec![TemplateSpan::Text(get_prefix), TemplateSpan::Type(p)]);
    // The Mapped type body: { [P in K as `get${P}`]: { a: P } }
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo::simple(db.intern_string("P")),
        constraint: k,
        template,
        name_type: Some(name_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    // Application alias: GetterMap<K>
    let app = db.application(db.lazy(crate::def::DefId(9001)), vec![k]);
    // Store display alias: mapped → app (simulates evaluate(Application) result)
    db.store_display_alias_preferring_application(mapped, app);
    // Index access: GetterMap<K>['get${K}']
    let idx = db.template_literal(vec![TemplateSpan::Text(get_prefix), TemplateSpan::Type(k)]);
    let access = db.index_access(mapped, idx);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(access);
    // The alias was stored, so the formatter uses the alias form, not the
    // expanded `{ [P in K as ...]: ... }` structural form.
    assert!(
        !result.contains("[P in K as"),
        "expected alias name form, not expanded mapped form, got: {result}"
    );
}

/// The alias-name display for remapped mapped index access must be
/// independent of the iteration variable name (`P`, `Q`, `X`, etc.).
/// A name-hardcoded check would silently fall back to structural form
/// when the alias uses a different variable name.
#[test]
fn remapped_mapped_type_alias_index_access_independent_of_iteration_var_name() {
    let db = TypeInterner::new();
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });

    // Build two structurally-distinct mapped types differing only in the
    // iteration variable name. Both get an Application alias stored.
    // The formatter must use the alias in both cases (structural rule, not name-matched).
    let make_aliased_access = |var_name: &str, def_id: u32| {
        let var = db.type_param(TypeParamInfo::simple(db.intern_string(var_name)));
        let prefix = db.intern_string("get");
        let name_type =
            db.template_literal(vec![TemplateSpan::Text(prefix), TemplateSpan::Type(var)]);
        let prop = db.intern_string("val");
        let template = db.object(vec![PropertyInfo::new(prop, var)]);
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo::simple(db.intern_string(var_name)),
            constraint: k,
            template,
            name_type: Some(name_type),
            readonly_modifier: None,
            optional_modifier: None,
        });
        let app = db.application(db.lazy(crate::def::DefId(def_id)), vec![k]);
        db.store_display_alias_preferring_application(mapped, app);
        let idx = db.template_literal(vec![TemplateSpan::Text(prefix), TemplateSpan::Type(k)]);
        db.index_access(mapped, idx)
    };

    let access_q = make_aliased_access("Q", 9002);
    let access_x = make_aliased_access("X", 9003);

    let mut fmt = TypeFormatter::new(&db);

    let result_q = fmt.format(access_q);
    assert!(
        !result_q.contains("[Q in K as"),
        "iteration var `Q`: expected alias form, got structural: {result_q}"
    );

    let result_x = fmt.format(access_x);
    assert!(
        !result_x.contains("[X in K as"),
        "iteration var `X`: expected alias form, got structural: {result_x}"
    );
}

/// A Mapped type without a display alias (simulating a raw mapped type not
/// produced from a known type alias) must format structurally for `IndexAccess`.
#[test]
fn remapped_mapped_type_without_alias_formats_structurally_in_index_access() {
    let db = TypeInterner::new();
    let k = db.type_param(TypeParamInfo {
        name: db.intern_string("K"),
        constraint: Some(TypeId::STRING),
        default: None,
        is_const: false,
    });
    let p = db.type_param(TypeParamInfo::simple(db.intern_string("P")));
    let prefix = db.intern_string("pfx");
    let name_type = db.template_literal(vec![TemplateSpan::Text(prefix), TemplateSpan::Type(p)]);
    let prop = db.intern_string("v");
    let template = db.object(vec![PropertyInfo::new(prop, p)]);
    // No display alias stored — structural display expected.
    let mapped = db.mapped(MappedType {
        type_param: TypeParamInfo::simple(db.intern_string("P")),
        constraint: k,
        template,
        name_type: Some(name_type),
        readonly_modifier: None,
        optional_modifier: None,
    });
    let idx = db.template_literal(vec![TemplateSpan::Text(prefix), TemplateSpan::Type(k)]);
    let access = db.index_access(mapped, idx);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(access);
    // Without an alias, the structural expanded form is shown.
    assert!(
        result.contains("[P in K as"),
        "expected structural mapped form when no alias stored, got: {result}"
    );
}

#[test]
fn format_this_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let this = db.this_type();
    assert_eq!(fmt.format(this), "this");
}

#[test]
fn format_infer_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let infer = db.infer(TypeParamInfo {
        name: db.intern_string("R"),
        constraint: None,
        default: None,
        is_const: false,
    });
    assert_eq!(fmt.format(infer), "infer R");
}

#[test]
fn format_unique_symbol() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let sym = db.unique_symbol(crate::types::SymbolRef(999));
    assert_eq!(fmt.format(sym), "unique symbol");
}

#[test]
fn format_no_infer_type() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // NoInfer<T> at the outermost layer is transparent — tsc strips a single
    // outer wrapper for display.
    let no_infer = db.no_infer(TypeId::STRING);
    assert_eq!(fmt.format(no_infer), "string");
}

#[test]
fn format_no_infer_inside_union_is_preserved() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // `NoInfer<string> | number` — NoInfer is a union member, not the
    // outermost type, so tsc keeps `NoInfer<>` in the displayed form.
    let no_infer = db.no_infer(TypeId::STRING);
    let union = db.union2(no_infer, TypeId::NUMBER);
    let result = fmt.format(union);
    assert!(
        result.contains("NoInfer<string>"),
        "expected `NoInfer<string>` in `{result}`"
    );
    assert!(result.contains("number"));
    assert!(result.contains(" | "));
}

#[test]
fn format_no_infer_outermost_with_union_inner_strips() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // `NoInfer<string | number>` — outer NoInfer wraps the union, so tsc
    // strips it and displays the inner union directly.
    let inner = db.union2(TypeId::STRING, TypeId::NUMBER);
    let no_infer = db.no_infer(inner);
    let result = fmt.format(no_infer);
    assert!(
        !result.contains("NoInfer"),
        "expected outer `NoInfer<>` to be stripped in `{result}`"
    );
    assert!(result.contains("string"));
    assert!(result.contains("number"));
    assert!(result.contains(" | "));
}

#[test]
fn format_nested_no_infer_keeps_inner() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // `NoInfer<NoInfer<string>>` — only the outermost `NoInfer<>` is
    // stripped; the nested one is preserved.
    let inner_no_infer = db.no_infer(TypeId::STRING);
    let outer_no_infer = db.no_infer(inner_no_infer);
    assert_eq!(fmt.format(outer_no_infer), "NoInfer<string>");
}

#[test]
fn format_no_infer_in_union_with_function_member() {
    // Regression for noInferUnionExcessPropertyCheck1.ts: tsc preserves
    // `NoInfer<>` on every union member (none of them is the outermost
    // type), even when a sibling member is a function whose return type
    // is also `NoInfer<>`. Order matters here: the union is sorted
    // structurally and the formatter must not strip the wrapper for
    // either arm.
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let x_atom = db.intern_string("x");
    let obj = db.object(vec![PropertyInfo {
        name: x_atom,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
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
    let no_infer_obj = db.no_infer(obj);
    let return_no_infer = db.no_infer(obj);
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: return_no_infer,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let union_id = db.union_preserve_members(vec![no_infer_obj, func]);
    let result = fmt.format(union_id);
    assert!(
        result.contains("NoInfer<{ x: string; }>"),
        "expected NoInfer<{{ x: string; }}> preserved on union member, got: {result}"
    );
    // Function-return NoInfer must also survive.
    assert!(
        result.contains("=> NoInfer<{ x: string; }>"),
        "expected NoInfer<> in function return, got: {result}"
    );
}

// =================================================================
// Generic application formatting
// =================================================================

#[test]
fn format_application_single_arg() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let base = db.lazy(crate::def::DefId(100));
    let app = db.application(base, vec![TypeId::NUMBER]);
    let result = fmt.format(app);
    // Without def store, base resolves to "Lazy(100)"
    assert!(
        result.contains("Lazy(100)"),
        "Expected 'Lazy(100)', got: {result}"
    );
    assert!(
        result.contains("<number>"),
        "Expected '<number>', got: {result}"
    );
}

#[test]
fn format_application_pads_missing_args_with_param_defaults() {
    // When the Application carries fewer args than the base's declared type
    // parameters, the formatter should pad missing trailing args with their
    // parameter defaults. Matches tsc's display: `Iterator<string>` renders
    // as `Iterator<string, any, any>` given `Iterator<T, TReturn = any,
    // TNext = any>`. Regression test for for-of29.ts.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let iter_name = db.intern_string("Iter");
    let t_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let treturn_param = TypeParamInfo {
        name: db.intern_string("TReturn"),
        constraint: None,
        default: Some(TypeId::ANY),
        is_const: false,
    };
    let tnext_param = TypeParamInfo {
        name: db.intern_string("TNext"),
        constraint: None,
        default: Some(TypeId::ANY),
        is_const: false,
    };
    let iter_body = db.object(vec![]); // structural body isn't relevant to the display test
    let iter_def = crate::def::DefinitionInfo::type_alias(
        iter_name,
        vec![t_param, treturn_param, tnext_param],
        iter_body,
    );
    let iter_def_id = def_store.register(iter_def);
    let base = db.lazy(iter_def_id);
    let app = db.application(base, vec![TypeId::STRING]);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(app);
    assert_eq!(
        result, "Iter<string, any, any>",
        "Missing trailing args must be padded with parameter defaults. Got: {result}"
    );
}

#[test]
fn format_iterable_iterator_elides_trailing_any_without_recorded_default() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let info = crate::def::DefinitionInfo::interface(
        db.intern_string("IterableIterator"),
        vec![
            TypeParamInfo {
                name: db.intern_string("T"),
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("TReturn"),
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("TNext"),
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        vec![],
    );
    let def_id = def_store.register(info);
    let app = db.application(
        db.lazy(def_id),
        vec![TypeId::STRING, TypeId::VOID, TypeId::ANY],
    );

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(app), "IterableIterator<string, void>");
}

#[test]
fn format_application_two_args() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let base = db.lazy(crate::def::DefId(200));
    let app = db.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
    let result = fmt.format(app);
    assert!(
        result.contains("<string, number>"),
        "Expected '<string, number>', got: {result}"
    );
}

#[test]
fn display_alias_does_not_repaint_preexisting_structural_type() {
    let db = TypeInterner::new();
    let prop = PropertyInfo::new(db.intern_string("p"), TypeId::NUMBER);
    let evaluated = db.object(vec![prop]);
    let type_param = db.type_param(TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });
    let app = db.application(db.lazy(crate::def::DefId(1)), vec![type_param]);

    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(evaluated);

    assert_eq!(
        result, "{ p: number; }",
        "A later generic application should not repaint an already-interned structural type"
    );
}

#[test]
fn skip_application_alias_names_suppresses_nested_application_display_alias() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let type_param = |name: &str| TypeParamInfo {
        name: db.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let merge_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("merge"),
        vec![type_param("base"), type_param("props")],
        TypeId::UNKNOWN,
    ));
    let omit_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Omit"),
        vec![type_param("T"), type_param("K")],
        TypeId::UNKNOWN,
    ));

    let p1 = db.object(vec![PropertyInfo::new(
        db.intern_string("p1"),
        TypeId::NUMBER,
    )]);
    let p2 = db.object(vec![PropertyInfo::new(
        db.intern_string("p2"),
        TypeId::NUMBER,
    )]);
    let merged = db.intersection(vec![p1, p2]);
    let merge_app = db.application(db.lazy(merge_def), vec![p1, p2]);
    db.store_display_alias(merged, merge_app);

    let omit_app = db.application(db.lazy(omit_def), vec![merged, db.literal_string("p2")]);
    let mut fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_skip_application_alias_names();

    assert_eq!(
        fmt.format(omit_app),
        "Omit<{ p1: number; } & { p2: number; }, \"p2\">"
    );
}

#[test]
fn skip_application_display_alias_chase_keeps_selected_application_name() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let type_param = |name: &str| TypeParamInfo {
        name: db.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let objectish_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Objectish"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let indirect_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("IndirectArrayish"),
        vec![type_param("U")],
        TypeId::UNKNOWN,
    ));
    let objectish_app = db.application(db.lazy(objectish_def), vec![TypeId::ANY]);
    let indirect_app = db.application(db.lazy(indirect_def), vec![TypeId::ANY]);
    db.store_display_alias(objectish_app, indirect_app);

    let mut default_fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(default_fmt.format(objectish_app), "IndirectArrayish<any>");

    let mut selected_app_fmt = TypeFormatter::new(&db)
        .with_def_store(&def_store)
        .with_skip_application_display_alias_chase();
    assert_eq!(selected_app_fmt.format(objectish_app), "Objectish<any>");
}

#[test]
fn concrete_display_alias_can_name_preexisting_structural_type() {
    let db = TypeInterner::new();
    let evaluated = db.object(vec![PropertyInfo::new(
        db.intern_string("p"),
        TypeId::NUMBER,
    )]);
    let app = db.application(
        db.lazy(crate::def::DefId(1)),
        vec![TypeId::NUMBER, TypeId::VOID, TypeId::UNKNOWN],
    );

    db.store_display_alias(evaluated, app);

    assert_eq!(
        db.get_display_alias(evaluated),
        Some(app),
        "Concrete application aliases should still name reused structural interface shapes"
    );
}

#[test]
fn preferred_application_display_alias_can_name_preexisting_structural_type() {
    let db = TypeInterner::new();
    let evaluated = db.object(vec![PropertyInfo::new(
        db.intern_string("p"),
        TypeId::NUMBER,
    )]);
    let app = db.application(
        db.lazy(crate::def::DefId(1)),
        vec![TypeId::NUMBER, TypeId::VOID, TypeId::UNKNOWN],
    );

    db.store_display_alias_preferring_application(evaluated, app);

    assert_eq!(
        db.get_display_alias(evaluated),
        Some(app),
        "Explicitly preferred application aliases should preserve nominal generic display"
    );
}

#[test]
fn application_display_alias_can_name_intermediate_application() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let type_param = |name: &str| TypeParamInfo {
        name: db.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    };
    let inner_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Inner"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let outer_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Outer"),
        vec![type_param("T")],
        TypeId::UNKNOWN,
    ));
    let one = db.literal_number(1.0);
    let inner_app = db.application(db.lazy(inner_def), vec![one]);
    let outer_app = db.application(db.lazy(outer_def), vec![one]);

    db.store_display_alias_preferring_application(inner_app, outer_app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(inner_app), "Outer<1>");
}

/// The empty object shape `{}` is a universally-shared interning target, but
/// some named generic interfaces/classes have empty bodies and still need a
/// display alias so their type arguments survive diagnostic rendering.
#[test]
fn display_alias_can_be_stored_for_empty_object_type() {
    let db = TypeInterner::new();
    let evaluated = db.object(vec![]);
    let app = db.application(db.lazy(crate::def::DefId(1)), vec![TypeId::UNKNOWN]);

    db.store_display_alias(evaluated, app);

    assert_eq!(
        db.get_display_alias(evaluated),
        Some(app),
        "Empty object applications may need a display_alias for named generic \
         interfaces/classes whose structural body is empty."
    );
}

/// Regression test for the `unknownType1` conformance failure: when a generic
/// mapped-type application (`T50<unknown>`) reduces to `{}` and is stored as
/// a `display_alias`, later diagnostics that reference `{}` (e.g.,
/// `let v6: {} = x` where `x: unknown`) would print the target type as
/// `T50<unknown>` instead of `{}`. The formatter must skip empty-object display
/// aliases only when the application base is a type alias.
#[test]
fn empty_object_formats_as_braces_after_mapped_reduction() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    // Simulate the result of evaluating `T50<unknown>`: an application over a
    // type alias with `unknown` as the sole argument reduces to `{}`.
    let evaluated = db.object(vec![]);
    let name = db.intern_string("T50");
    let def_id = def_store.register(crate::def::DefinitionInfo::type_alias(
        name,
        vec![],
        evaluated,
    ));
    let app = db.application(db.lazy(def_id), vec![TypeId::UNKNOWN]);

    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(evaluated);
    assert_eq!(
        result, "{}",
        "Empty object must format as `{{}}` even when a generic application \
         has reduced to the same interned shape."
    );
}

#[test]
fn empty_object_interface_application_preserves_type_args() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let evaluated = db.object(vec![]);
    let name = db.intern_string("AsyncGenerator");
    let info = crate::def::DefinitionInfo::interface(
        name,
        vec![
            TypeParamInfo {
                name: db.intern_string("T"),
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("TReturn"),
                constraint: None,
                default: None,
                is_const: false,
            },
            TypeParamInfo {
                name: db.intern_string("TNext"),
                constraint: None,
                default: None,
                is_const: false,
            },
        ],
        vec![],
    );
    let def_id = def_store.register(info);
    def_store.register_type_to_def(evaluated, def_id);
    let app = db.application(
        db.lazy(def_id),
        vec![TypeId::NUMBER, TypeId::VOID, TypeId::UNKNOWN],
    );

    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(evaluated),
        "AsyncGenerator<number, void, unknown>",
        "Named generic interfaces with empty structural bodies must keep their \
         application display."
    );
}

/// Regression test for `undefinedAssignableToEveryType`: the lib resolution
/// path registers `Promise`'s interface def against the canonical empty `{}`
/// TypeId without setting `instance_shape` and without a `display_alias`.
/// The interface has type params (`Promise<T>`), so the formatter would
/// reach the bare-type-param fallback and render every user-written `{}`
/// annotation as `Promise<T>`. The formatter must render `{}` structurally
/// when there is no concrete instantiation to display.
#[test]
fn empty_object_does_not_repaint_as_generic_interface_without_display_alias() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let evaluated = db.object(vec![]);
    let name = db.intern_string("Promise");
    let info = crate::def::DefinitionInfo::interface(
        name,
        vec![TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        vec![],
    );
    let def_id = def_store.register(info);
    def_store.register_type_to_def(evaluated, def_id);
    // Note: no `store_display_alias`. This mirrors the lib-resolution
    // registration path for `Promise`, which never sets a display alias for
    // the canonical empty `{}` TypeId.

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(evaluated),
        "{}",
        "A user-written `{{}}` annotation must format as `{{}}`, not as \
         `Promise<T>`, when a generic interface def has been registered \
         against the universal empty-object TypeId without provenance."
    );

    // The same shape must hold inside derived TypeData, e.g. inside a
    // function return type. `var j: () => {} = undefined` previously
    // rendered the target as `() => Promise<T>`.
    let func = db.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: evaluated,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let mut fmt2 = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt2.format(func),
        "() => {}",
        "Empty `{{}}` inside a function return type must render structurally \
         even when an unrelated generic interface is keyed on its TypeId."
    );

    // A non-generic empty interface (`interface I {}`) must still display
    // its name when registered against the empty-object TypeId.
    let db2 = TypeInterner::new();
    let store2 = crate::def::DefinitionStore::new();
    let evaluated2 = db2.object(vec![]);
    let i_name = db2.intern_string("I");
    let i_info = crate::def::DefinitionInfo::interface(i_name, vec![], vec![]);
    let i_def = store2.register(i_info);
    store2.register_type_to_def(evaluated2, i_def);
    let mut fmt3 = TypeFormatter::new(&db2).with_def_store(&store2);
    assert_eq!(
        fmt3.format(evaluated2),
        "I",
        "A non-generic empty interface registered against the empty-object \
         TypeId must keep its name."
    );
}

/// Second half of the `unknownType1` regression: the type-alias `T52` is
/// declared as `type T52 = T50<unknown>`, and the checker registers the
/// evaluated body `{}` against `T52` via `register_type_to_def`. Without
/// the formatter guard, every user-written `{}` annotation in diagnostics
/// would pick up `T52` (or any sibling def that also reduces to `{}`).
/// The formatter must render `{}` structurally regardless of what def is
/// keyed on the empty-object `TypeId`.
#[test]
fn empty_object_formats_as_braces_when_def_registered() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let evaluated = db.object(vec![]);
    let name = db.intern_string("T52");
    let info = crate::def::DefinitionInfo::type_alias(name, vec![], evaluated);
    let def_id = def_store.register(info);
    def_store.register_type_to_def(evaluated, def_id);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    let result = fmt.format(evaluated);
    assert_eq!(
        result, "{}",
        "Empty object must format as `{{}}` even when a type alias's body \
         happens to reduce to the same interned shape (would otherwise \
         repaint every `{{}}` annotation with the alias name)."
    );
}

#[test]
fn structural_display_alias_can_replace_generic_helper_alias() {
    let db = TypeInterner::new();
    let app = db.application(db.lazy(crate::def::DefId(1)), vec![TypeId::STRING]);
    let evaluated = db.object(vec![PropertyInfo::new(
        db.intern_string("p"),
        TypeId::NUMBER,
    )]);
    let structural_alias = db.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER]);

    db.store_display_alias(evaluated, app);
    db.store_display_alias(evaluated, structural_alias);

    let mut fmt = TypeFormatter::new(&db);
    let result = fmt.format(evaluated);

    assert_eq!(
        result, "string | number",
        "Structural display provenance should replace a generic helper alias"
    );
}

#[test]
fn string_intrinsic_display_alias_keeps_resolved_intrinsic_surface() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let alias_name = db.intern_string("Wrapper");
    let def_id = def_store.register(crate::def::DefinitionInfo::interface(
        alias_name,
        vec![TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        vec![],
    ));
    let app = db.application(db.lazy(def_id), vec![TypeId::STRING]);
    let evaluated = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);

    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(evaluated),
        "Uppercase<string>",
        "Resolved string intrinsics should not be repainted through alias provenance"
    );
}

#[test]
fn template_literal_display_alias_keeps_resolved_pattern_surface() {
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();
    let alias_name = db.intern_string("Wrapper");
    let def_id = def_store.register(crate::def::DefinitionInfo::interface(
        alias_name,
        vec![TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        }],
        vec![],
    ));
    let app = db.application(db.lazy(def_id), vec![TypeId::STRING]);
    let evaluated = db.template_literal(vec![
        TemplateSpan::Text(db.intern_string("AA")),
        TemplateSpan::Type(db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING)),
    ]);

    db.store_display_alias(evaluated, app);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(evaluated),
        "`AA${Uppercase<string>}`",
        "Resolved template literal patterns should not be repainted through alias provenance"
    );
}

// =================================================================
// Callable type formatting
// =================================================================

#[test]
fn format_callable_single_call_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let callable = db.callable(CallableShape {
        call_signatures: vec![CallSignature {
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
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let result = fmt.format(callable);
    // Single call sig with no props/index = arrow-style
    assert!(result.contains("x: number"));
    assert!(result.contains("=> string"));
}

/// Test for conformance issue: { <S>() : S[]; } should format as <S>() => S[]
#[test]
fn format_callable_generic_single_call_signature_arrow_syntax() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create type param S
    let s_param = TypeParamInfo {
        name: db.intern_string("S"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_type = db.type_param(s_param);

    // Create S[] array type
    let s_array = db.array(s_type);

    // Create callable: { <S>() : S[]; } - single call signature, no params, returns S[]
    let callable = db.callable(CallableShape {
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
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let result = fmt.format(callable);
    // Single call signature with no props/construct/index should use arrow syntax
    // Expected: <S>() => S[]
    // NOT: { <S>() : S[];; } (object syntax with double semicolons)
    assert!(
        result.contains("=>"),
        "Expected arrow syntax for single call signature callable, got: {result}"
    );
    assert!(
        !result.contains("{{"),
        "Should NOT use object braces for single call signature callable, got: {result}"
    );
    assert!(
        !result.contains(";;"),
        "Should NOT have double semicolons, got: {result}"
    );
    assert_eq!(
        result, "<S>() => S[]",
        "Exact match for single call signature callable"
    );
}

#[test]
fn format_callable_single_construct_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Anonymous callable with single construct signature should use arrow notation:
    // `new <T>(x: T, y: T) => string` instead of `{ new <T>(x: T, y: T): string; }`
    let type_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = db.type_param(type_param);

    let callable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![type_param],
            params: vec![
                ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: t_type,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(db.intern_string("y")),
                    type_id: t_type,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let result = fmt.format(callable);
    // Single construct sig with no props/index = arrow-style with 'new' prefix
    assert!(
        result.contains("new") && result.contains("<T>") && result.contains("=> string"),
        "Single construct signature should use arrow notation like 'new <T>(x: T, y: T) => string', got: {result}"
    );
    // Should NOT have braces (object literal format)
    assert!(
        !result.starts_with('{'),
        "Single construct signature should NOT use object notation, got: {result}"
    );
}

