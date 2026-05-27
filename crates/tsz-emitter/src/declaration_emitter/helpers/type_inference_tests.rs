use super::DeclarationEmitter;
use std::sync::atomic::{AtomicU64, Ordering};
use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::construction::TypeInterner;

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

fn first_function_declared_return_identifier_type_text(source: &str) -> Option<String> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    parser.arena.nodes.iter().find_map(|node| {
        (node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
            .then(|| parser.arena.get_function(node))
            .flatten()
            .and_then(|func| emitter.function_body_declared_return_identifier_type_text(func))
    })
}

#[test]
fn function_return_surface_reuses_returned_identifier_mapped_annotation() {
    let text = first_function_declared_return_identifier_type_text(
        r#"
type PartialProperties<T, K extends keyof T> = Partial<Pick<T, K>>;
export function sample<T extends { prop: string }>(a: T) {
    const value: { [K in keyof PartialProperties<T, "prop">]: PartialProperties<T, "prop">[K]; } = null as any;
    return value;
}
"#,
    )
    .expect("return identifier type text");

    assert!(text.contains("[K in keyof PartialProperties<T, \"prop\">]"));
    assert!(text.contains("PartialProperties<T, \"prop\">[K]"));
}

#[test]
fn function_return_surface_reuses_renamed_returned_identifier_mapped_annotation() {
    let text = first_function_declared_return_identifier_type_text(
        r#"
type Picked<U, Q extends keyof U> = Pick<U, Q>;
export function sample<U extends { name: string }>(input: U) {
    const value: { [Q in keyof Picked<U, "name">]: Picked<U, "name">[Q]; } = null as any;
    return value;
}
"#,
    )
    .expect("return identifier type text");

    assert!(text.contains("[Q in keyof Picked<U, \"name\">]"));
    assert!(text.contains("Picked<U, \"name\">[Q]"));
}

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
fn string_literal_union_is_preferred_when_solver_widens_to_string() {
    assert!(DeclarationEmitter::string_literal_union_widens_to_string(
        "\"ROAAAAR!\" | \"yip yip!\"",
        "string",
    ));
    assert!(!DeclarationEmitter::string_literal_union_widens_to_string(
        "\"ROAAAAR!\"",
        "string",
    ));
    assert!(!DeclarationEmitter::string_literal_union_widens_to_string(
        "\"ROAAAAR!\" | number",
        "string",
    ));
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
fn types_versions_self_back_reference_detection_requires_package_root_reexport() {
    let temp_id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "tsz-types-versions-self-ref-{}-{}",
        std::process::id(),
        temp_id
    ));
    let package_root = root.join("node_modules").join("ext");
    let types_dir = package_root.join("ts3.1");
    std::fs::create_dir_all(&types_dir).expect("create typesVersions dir");
    std::fs::write(
        package_root.join("package.json"),
        r#"{
            "name": "ext",
            "version": "1.0.0",
            "typesVersions": {
                ">=3.1.0-0": { "*": ["ts3.1/*"] }
            }
        }"#,
    )
    .expect("write package json");
    std::fs::write(types_dir.join("index.d.ts"), r#"export * from "../";"#)
        .expect("write self back-reference");

    assert!(
        DeclarationEmitter::package_root_has_types_versions_self_back_reference(&package_root),
        "Expected root typesVersions index re-exporting the parent package root to be detected"
    );

    std::fs::write(types_dir.join("index.d.ts"), r#"export * from "../other";"#)
        .expect("rewrite non-root re-export");
    assert!(
        !DeclarationEmitter::package_root_has_types_versions_self_back_reference(&package_root),
        "Subpath re-exports should not make the package root fall back to any"
    );

    let subpath_types_dir = package_root.join("ts3.1").join("sub");
    std::fs::create_dir_all(&subpath_types_dir).expect("create subpath typesVersions dir");
    std::fs::write(
        package_root.join("package.json"),
        r#"{
            "name": "ext",
            "version": "1.0.0",
            "typesVersions": {
                ">=3.1.0-0": { "sub/*": ["ts3.1/sub/*"] }
            }
        }"#,
    )
    .expect("rewrite package json");
    std::fs::write(
        subpath_types_dir.join("index.d.ts"),
        r#"export * from "../";"#,
    )
    .expect("write subpath back-reference");
    assert!(
        !DeclarationEmitter::package_root_has_types_versions_self_back_reference(&package_root),
        "Subpath-only typesVersions mappings should not make root imports fall back to any"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn types_versions_mapped_index_path_prints_package_root_specifier() {
    let temp_id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "tsz-types-versions-public-specifier-{}-{}",
        std::process::id(),
        temp_id
    ));
    let package_root = root.join("node_modules").join("ext");
    let types_dir = package_root.join("ts3.1");
    std::fs::create_dir_all(&types_dir).expect("create typesVersions dir");
    std::fs::write(
        package_root.join("package.json"),
        r#"{
            "name": "ext",
            "version": "1.0.0",
            "types": "index",
            "typesVersions": {
                ">=3.1.0-0": { "index": ["ts3.1/index"] }
            }
        }"#,
    )
    .expect("write package json");
    std::fs::write(types_dir.join("index.d.ts"), r#"export * from "../other";"#)
        .expect("write mapped declaration");
    std::fs::write(package_root.join("other.d.ts"), r#"export interface A2 {}"#)
        .expect("write reexport target declaration");

    let parser = ParserState::new("main.ts".to_string(), String::new());
    let emitter = DeclarationEmitter::new(&parser.arena);
    let current_path = root.join("main.ts");
    let mapped_path = types_dir.join("index.d.ts");

    assert_eq!(
        emitter.package_specifier_for_node_modules_path(
            current_path.to_str().expect("current path utf-8"),
            mapped_path.to_str().expect("mapped path utf-8"),
        ),
        Some("ext".to_string())
    );
    assert_eq!(
        emitter.package_specifier_for_node_modules_path(
            current_path.to_str().expect("current path utf-8"),
            package_root
                .join("other.d.ts")
                .to_str()
                .expect("other path utf-8"),
        ),
        Some("ext".to_string())
    );

    let _ = std::fs::remove_dir_all(&root);
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
fn object_union_arms_without_methods_are_expanded() {
    // tsc normalizes object literals in a union upon widening regardless of
    // whether any arm contains a method: the property-only arm gains
    // `b?: undefined`.
    let mut types = vec![
        "{\n    a: number;\n}".to_string(),
        "{\n    a: number;\n    b: string;\n}".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    a: number;\n    b?: undefined;\n}".to_string(),
            "{\n    a: number;\n    b: string;\n}".to_string(),
        ]
    );
}

#[test]
fn object_union_three_property_only_arms_cross_normalize() {
    // Three property-only arms with differing keys each gain `?: undefined`
    // for every sibling key they omit (the widened source-array union shape).
    let mut types = vec![
        "{\n    x: number;\n}".to_string(),
        "{\n    x: number;\n    y: number;\n}".to_string(),
        "{\n    x: number;\n    err: boolean;\n}".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    x: number;\n    y?: undefined;\n    err?: undefined;\n}".to_string(),
            "{\n    x: number;\n    y: number;\n    err?: undefined;\n}".to_string(),
            "{\n    x: number;\n    err: boolean;\n    y?: undefined;\n}".to_string(),
        ]
    );
}

#[test]
fn object_union_empty_arm_and_property_arms_all_cross_normalize() {
    // `[{ a: 1, b: 2 }, { a: "abc" }, {}]`-shaped union: the empty arm gains
    // every key as optional-undefined, and the partial arms gain the keys
    // they omit. Verifies the empty-arm path no longer skips the other arms.
    let mut types = vec![
        "{\n    a: number;\n    b: number;\n}".to_string(),
        "{\n    a: string;\n}".to_string(),
        "{}".to_string(),
    ];

    DeclarationEmitter::expand_object_union_arms_from_sibling_properties(&mut types);

    assert_eq!(
        types,
        vec![
            "{\n    a: number;\n    b: number;\n}".to_string(),
            "{\n    a: string;\n    b?: undefined;\n}".to_string(),
            "{\n    a?: undefined;\n    b?: undefined;\n}".to_string(),
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
fn package_root_fallback_matches_export_star_package_internals() {
    let root = std::env::temp_dir().join(format!(
        "tsz-emitter-package-root-fallback-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("repo/node_modules/typescript-fsa");
    let source_dir = package_root.join("src");
    std::fs::create_dir_all(&source_dir).expect("create package dirs");
    std::fs::write(
        package_root.join("package.json"),
        r#"{"name":"typescript-fsa","version":"1.0.0"}"#,
    )
    .expect("write package json");
    let impl_path = source_dir.join("impl.d.ts");
    std::fs::write(&impl_path, "export enum A { Val }\n").expect("write impl");

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    parser.parse_source_file();
    let emitter = DeclarationEmitter::new(&parser.arena);
    let impl_path = impl_path.to_string_lossy();

    assert!(emitter.node_modules_package_contains_import_specifier(&impl_path, "typescript-fsa"));
    assert!(emitter.package_json_name_matches_import_specifier(&impl_path, "typescript-fsa"));
    assert_eq!(
        DeclarationEmitter::rewrite_relative_import_type_specifiers(
            r#"import("../cache/typescript-fsa/src/impl").A"#,
            "typescript-fsa",
        ),
        r#"import("typescript-fsa").A"#
    );
    assert_eq!(
        DeclarationEmitter::rewrite_relative_import_type_specifiers(
            r#"import("@raymondfeng/pkg2/dist/secondary").IdType"#,
            "@raymondfeng/pkg2",
        ),
        r#"import("@raymondfeng/pkg2/dist/secondary").IdType"#
    );

    std::fs::write(
        package_root.join("package.json"),
        r#"{"name":"typescript-fsa","exports":"./index.d.ts"}"#,
    )
    .expect("rewrite package json");
    assert!(!emitter.node_modules_package_contains_import_specifier(&impl_path, "typescript-fsa"));
    assert!(!emitter.package_json_name_matches_import_specifier(&impl_path, "typescript-fsa"));

    let _ = std::fs::remove_dir_all(&root);
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

#[test]
fn tuple_item_lookup_mapped_type_expands_literal_keys() {
    let input = r#"{
    [Item in readonly [{
    readonly name: "a";
}, {
    readonly name: "b";
}][number] as Item["name"]]: Item;
}"#;

    assert_eq!(
        DeclarationEmitter::expand_tuple_item_lookup_mapped_type_text(input),
        Some(
            "{\n    a: {\n        readonly name: \"a\";\n    };\n    b: {\n        readonly name: \"b\";\n    };\n}"
            .to_string()
        )
    );
}

#[test]
fn tuple_item_lookup_mapped_type_expands_inline_compact_string_key() {
    let input = r#"{
    [Item in readonly [{
    readonly name: "a";
}, {
    readonly name: "b";
}][number]as Item[string]]: Item;
}"#;

    assert_eq!(
        DeclarationEmitter::expand_tuple_item_lookup_mapped_type_text(input),
        Some(
            "{\n    a: {\n        readonly name: \"a\";\n    };\n    b: {\n        readonly name: \"b\";\n    };\n}"
                .to_string()
        )
    );
}

#[test]
fn tuple_item_lookup_mapped_type_expands_compact_string_key() {
    let input = r#"{ [Item in readonly [{
    readonly name: "a";
}, {
    readonly name: "b";
}][number]as Item[string]]: Item }"#;

    assert_eq!(
        DeclarationEmitter::expand_tuple_item_lookup_mapped_type_text(input),
        Some(
            "{\n    a: {\n        readonly name: \"a\";\n    };\n    b: {\n        readonly name: \"b\";\n    };\n}"
                .to_string()
        )
    );
}

#[test]
fn isomorphic_mapped_argument_unwraps_tuple_and_array_wrappers() {
    assert_eq!(
        DeclarationEmitter::infer_unwrapped_isomorphic_mapped_argument_text(
            "[Box<number>, Box<string>, ...Box<boolean>[]]",
            "Box",
        ),
        Some("[number, string, ...boolean[]]".to_string())
    );
    assert_eq!(
        DeclarationEmitter::infer_unwrapped_isomorphic_mapped_argument_text(
            "[Box<number>, Box<string>, ...Box<boolean>]",
            "Box",
        ),
        Some("[number, string, ...boolean[]]".to_string())
    );
    assert_eq!(
        DeclarationEmitter::infer_unwrapped_isomorphic_mapped_argument_text("Box<number>[]", "Box"),
        Some("number[]".to_string())
    );
}

#[test]
fn partial_argument_inference_restores_required_public_surface() {
    assert_eq!(
        DeclarationEmitter::infer_required_from_partial_argument_text(
            "[number | undefined, string?, ...boolean[]]",
        ),
        Some("[number, string, ...boolean[]]".to_string())
    );
    assert_eq!(
        DeclarationEmitter::infer_required_from_partial_argument_text(
            "[number | undefined, string?, ...boolean]",
        ),
        Some("[number, string, ...boolean[]]".to_string())
    );
    assert_eq!(
        DeclarationEmitter::infer_required_from_partial_argument_text(
            "{ a: number | undefined; b?: string[]; }",
        ),
        Some("{\n    a: number;\n    b: string[];\n}".to_string())
    );
}

#[test]
fn declared_call_return_inverts_isomorphic_mapped_tuple_argument() {
    let source = r#"
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare function unboxify<T>(x: Boxified<T>): T;
declare let x10: [Box<number>, Box<string>, ...Box<boolean>[]];
let y10 = unboxify(x10);
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    let call_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .find_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::CALL_EXPRESSION).then_some(NodeIndex(idx as u32))
        })
        .expect("missing call expression");

    assert_eq!(
        emitter.call_expression_declared_return_type_text(call_idx),
        Some("[number, string, ...boolean[]]".to_string())
    );
}
