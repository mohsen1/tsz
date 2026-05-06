use super::DeclarationEmitter;
use tsz_parser::parser::ParserState;

#[test]
fn simultaneous_word_replacement_does_not_rewrite_inserted_import_paths() {
    let rewritten = DeclarationEmitter::replace_whole_words_in_text(
        "A | B",
        &[
            ("A".to_string(), "import(\"./B\").A".to_string()),
            ("B".to_string(), "import(\"./C\").B".to_string()),
        ],
    );

    assert_eq!(rewritten, "import(\"./B\").A | import(\"./C\").B");
}

#[test]
fn simultaneous_word_replacement_does_not_chain_type_parameter_substitutions() {
    let rewritten = DeclarationEmitter::replace_whole_words_in_text(
        "T | U",
        &[
            ("T".to_string(), "Promise<U>".to_string()),
            ("U".to_string(), "string".to_string()),
        ],
    );

    assert_eq!(rewritten, "Promise<U> | string");
}

#[test]
fn word_replacement_skips_template_literal_text_segments() {
    let rewritten = DeclarationEmitter::replace_whole_words_in_text(
        "`Kind-${string}` | Kind | `${Kind}`",
        &[("Kind".to_string(), "import(\"nested\").Kind".to_string())],
    );

    assert_eq!(
        rewritten,
        "`Kind-${string}` | import(\"nested\").Kind | `${import(\"nested\").Kind}`"
    );
}

#[test]
fn word_search_skips_template_literal_text_segments() {
    assert!(!DeclarationEmitter::contains_whole_word_in_text(
        "`Kind-${string}`",
        "Kind",
    ));
    assert!(DeclarationEmitter::contains_whole_word_in_text(
        "`${Kind}`",
        "Kind",
    ));
}

#[test]
fn import_type_text_helpers_accept_single_quoted_specifiers() {
    let parser = ParserState::new("test.ts".to_string(), String::new());
    let emitter = DeclarationEmitter::new(&parser.arena);

    assert!(DeclarationEmitter::type_text_starts_with_import_type(
        "import('nested').NestedProps"
    ));
    assert!(DeclarationEmitter::type_text_contains_import_type(
        "[import('nested').NestedProps]"
    ));
    assert_eq!(
        emitter.parse_import_type_text("import('nested').NestedProps"),
        Some(("nested".to_string(), "NestedProps".to_string()))
    );
}

#[test]
fn empty_object_union_arm_expands_missing_quoted_property() {
    let mut types = vec!["{\n    \"a-b\": string;\n}".to_string(), "{}".to_string()];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    \"a-b\": string;\n}".to_string(),
            "{\n    \"a-b\"?: undefined;\n}".to_string(),
        ]
    );
}

#[test]
fn empty_object_union_arm_expands_with_mixed_non_object_arm() {
    let mut types = vec![
        "{\n    foo: number;\n}".to_string(),
        "{}".to_string(),
        "number".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    foo: number;\n}".to_string(),
            "{\n    foo?: undefined;\n}".to_string(),
            "number".to_string(),
        ]
    );
}

#[test]
fn object_union_arms_expand_missing_sibling_properties_and_methods() {
    let mut types = vec![
        "{\n    foo: number;\n    m(): void;\n}".to_string(),
        "{\n    bar: number;\n}".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    foo: number;\n    m(): void;\n    bar?: undefined;\n}".to_string(),
            "{\n    bar: number;\n    foo?: undefined;\n    m?: undefined;\n}".to_string(),
        ]
    );
}

#[test]
fn optional_method_triggers_object_union_sibling_expansion() {
    let mut types = vec![
        "{\n    m?(): void;\n}".to_string(),
        "{\n    value: number;\n}".to_string(),
        "string".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    m?(): void;\n    value?: undefined;\n}".to_string(),
            "{\n    value: number;\n    m?: undefined;\n}".to_string(),
            "string".to_string(),
        ]
    );
}

#[test]
fn object_union_arms_without_methods_are_not_expanded() {
    let mut types = vec![
        "{\n    a: number;\n}".to_string(),
        "{\n    a: number;\n    b: string;\n}".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    a: number;\n}".to_string(),
            "{\n    a: number;\n    b: string;\n}".to_string(),
        ]
    );
}

#[test]
fn object_union_property_scan_ignores_nested_members() {
    assert_eq!(
        DeclarationEmitter::object_type_top_level_member_names(
            "{\n    outer: {\n        inner: string;\n    };\n}",
            true,
        ),
        vec!["outer".to_string()]
    );
}

#[test]
fn object_type_property_name_scan_handles_quoted_colons_and_skips_methods() {
    assert_eq!(
        DeclarationEmitter::object_type_property_name_from_line("\"a:b\": string;"),
        Some("\"a:b\"".to_string())
    );
    assert_eq!(
        DeclarationEmitter::object_type_property_name_from_line("foo(x: number): void;"),
        None
    );
    assert_eq!(
        DeclarationEmitter::object_type_property_name_from_line("readonly \"a:b\"?: string;"),
        Some("\"a:b\"".to_string())
    );
}

#[test]
fn node_modules_package_path_match_accepts_root_declaration_files() {
    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    parser.parse_source_file();
    let emitter = DeclarationEmitter::new(&parser.arena);

    assert!(
        emitter.node_modules_package_path_matches_import_specifier(
            "/repo/node_modules/umd.d.ts",
            "umd"
        )
    );
    assert!(emitter.node_modules_package_path_matches_import_specifier(
        "/repo/node_modules/umd/index.d.ts",
        "umd"
    ));
    assert!(emitter.node_modules_package_path_matches_import_specifier(
        "/repo/node_modules/umd/sub/index.d.ts",
        "umd/sub"
    ));
    assert!(!emitter.node_modules_package_path_matches_import_specifier(
        "/repo/node_modules/umd/sub/index.d.ts",
        "umd"
    ));
}

#[test]
fn public_module_specifier_combines_relative_default_import_target() {
    assert_eq!(
        DeclarationEmitter::combine_public_module_specifier("@ts-bug/core/utils", "./SvgIcon"),
        Some("@ts-bug/core/SvgIcon".to_string())
    );
    assert_eq!(
        DeclarationEmitter::combine_public_module_specifier("pkg/sub/utils", "../Icon"),
        Some("pkg/Icon".to_string())
    );
    assert_eq!(
        DeclarationEmitter::combine_public_module_specifier("./utils", "./SvgIcon"),
        None
    );
}
