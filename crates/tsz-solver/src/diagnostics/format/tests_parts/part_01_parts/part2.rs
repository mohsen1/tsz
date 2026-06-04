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
