use super::DeclarationEmitter;
use std::sync::Arc;
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

fn emit_test_dts_with_binding(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let current_arena = Arc::new(parser.arena.clone());

    let mut emitter =
        DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);
    emitter.set_current_arena(current_arena, "test.ts".to_string());
    emitter.emit(root)
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
fn conditional_object_literal_union_preserves_branch_order() {
    let types = vec![
        "{\n    a: number;\n    b: number;\n}".to_string(),
        "{}".to_string(),
    ];

    let normalized = DeclarationEmitter::normalized_object_literal_union_text(types)
        .expect("object literal arms should normalize");

    assert_eq!(
        normalized,
        "{\n    a: number;\n    b: number;\n} | {\n    a?: undefined;\n    b?: undefined;\n}"
    );
}

#[test]
fn conditional_empty_then_object_literal_union_preserves_branch_order() {
    let types = vec![
        "{}".to_string(),
        "{\n    a: number;\n    b: number;\n}".to_string(),
    ];

    let normalized = DeclarationEmitter::normalized_object_literal_union_text(types)
        .expect("object literal arms should normalize");

    assert_eq!(
        normalized,
        "{\n    a?: undefined;\n    b?: undefined;\n} | {\n    a: number;\n    b: number;\n}"
    );
}

#[test]
fn nested_object_union_member_properties_cross_normalize_from_source_arms() {
    let source = r#"
declare const flag: boolean;
let result = [
    { kind: "first", pos: { x: 1, y: 2 } },
    { kind: "second", pos: flag ? { a: "value" } : { b: 3 } },
];
"#;

    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("x: number;")
            && output.contains("y: number;")
            && output.contains("a?: undefined;")
            && output.contains("b?: undefined;"),
        "expected first nested source arm to gain sibling optional members: {output}"
    );
    assert!(
        output.contains("a: string;")
            && output.contains("b: number;")
            && output.contains("x?: undefined;")
            && output.contains("y?: undefined;"),
        "expected conditional nested source arms to stay structured through normalization: {output}"
    );
}

#[test]
fn conditional_object_literal_union_widens_member_literals() {
    let source = r#"
declare const flag: boolean;
let result = flag ? { a: "x", b: 0, c: true } : {};
"#;

    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("a: string;"),
        "expected string literal member to widen through source arm summary: {output}"
    );
    assert!(
        output.contains("b: number;"),
        "expected numeric literal member to widen through source arm summary: {output}"
    );
    assert!(
        output.contains("c: boolean;"),
        "expected boolean literal member to widen through source arm summary: {output}"
    );
}

#[test]
fn generic_rest_identity_object_literal_union_preserves_argument_order() {
    let source = r#"
declare function f<T>(...items: T[]): T;
let result = f({}, { a: "abc" }, { a: 1, b: 2 });
"#;

    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains(
            "declare let result: {\n    a?: undefined;\n    b?: undefined;\n} | {\n    a: string;\n    b?: undefined;\n} | {\n    a: number;\n    b: number;\n};"
        ),
        "expected generic rest identity object union in argument order: {output}"
    );
}

#[test]
fn generic_rest_identity_prefers_declared_object_literal_surface() {
    let source = r#"
declare function f<T>(...items: T[]): T;
declare let data: { a: 1, b: "abc", c: true };
let first = f(data, { a: 2 });
let second = f({ a: 2 }, data);
"#;

    let output = emit_test_dts_with_binding(source);

    for name in ["first", "second"] {
        let expected =
            format!("declare let {name}: {{\n    a: 1;\n    b: \"abc\";\n    c: true;\n}};");
        assert!(
            output.contains(&expected),
            "expected `{name}` to reuse declared literal object surface: {output}"
        );
    }
}

#[test]
fn object_spread_projection_prepends_own_members_to_declared_union_arms() {
    let own_members = vec!["z: number".to_string()];

    let projected = DeclarationEmitter::prepend_object_members_to_type_literal_text(
        "{\n    a: string;\n    b: string;\n}",
        &own_members,
        0,
    );

    assert_eq!(
        projected,
        Some("{\n    z: number;\n    a: string;\n    b: string;\n}".to_string())
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

#[test]
fn declared_call_return_uses_receiver_interface_member_signature() {
    let source = r#"
interface Widget {
    id: number;
}
type ExtendedWidget<T> = { id: number; ext: T };
interface WidgetFactory {
    make<T>(x: T): ExtendedWidget<T extends unknown ? Widget : undefined>;
}
declare const Widget: WidgetFactory;
import * as ns from "mod";
const ImportedWidget = ns.default;
let y = Widget.make({ label: "ok" });
let z = ImportedWidget.make({ label: "ok" });
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

    assert!(
        emitter
            .call_expression_declared_return_type_text(call_idx)
            .is_some_and(|text| text.contains("ExtendedWidget")),
    );
    let imported_call_idx = parser
        .arena
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            (node.kind == syntax_kind_ext::CALL_EXPRESSION).then_some(NodeIndex(idx as u32))
        })
        .nth(1)
        .expect("missing imported call expression");
    let imported_call = parser
        .arena
        .get_call_expr(
            parser
                .arena
                .get(imported_call_idx)
                .expect("imported call node"),
        )
        .expect("imported call data");
    assert!(
        emitter
            .call_receiver_default_import_alias(imported_call.expression)
            .is_some_and(|(_, module)| module == "mod"),
        "default import alias detection failed"
    );
}

#[test]
fn declared_call_return_substitutes_receiver_type_parameters_before_member_type_parameters() {
    let source = r#"
type InferPipe<State, Fn> =
    Fn extends (value: State) => unknown ? (value: State) => ReturnType<Fn> : never;
interface PipeBox<State> {
    pipe<Fn extends (value: State) => unknown>(fn: Fn): PipeBox<InferPipe<State, Fn>>;
}
declare const source: PipeBox<string>;
declare const parseValue: (value: string) => number;
let out = source.pipe(parseValue);
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

    assert!(emitter.call_expression_declared_return_has_source_conditional_alias(call_idx));
    assert_eq!(
        emitter.call_expression_declared_return_type_text(call_idx),
        Some("PipeBox<InferPipe<string, (value: string) => number>>".to_string())
    );
}

#[test]
fn declared_call_return_substitutes_receiver_type_parameters_with_renamed_binders() {
    let source = r#"
type Next<Input, Handler> =
    Handler extends (item: Input) => unknown ? (item: Input) => ReturnType<Handler> : never;
interface Chain<Input> {
    map<Handler extends (item: Input) => unknown>(handler: Handler): Chain<Next<Input, Handler>>;
}
declare const chain: Chain<number>;
declare const handler: (item: number) => string;
let out = chain.map(handler);
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

    assert!(emitter.call_expression_declared_return_has_source_conditional_alias(call_idx));
    assert_eq!(
        emitter.call_expression_declared_return_type_text(call_idx),
        Some("Chain<Next<number, (item: number) => string>>".to_string())
    );
}

#[test]
fn instantiation_expression_source_surface_detects_unresolved_declaration_text() {
    assert!(
        !DeclarationEmitter::instantiated_source_type_needs_semantic_surface(
            r#"() => { value: T extends Other ? "O" : "N"; }"#
        )
    );
    assert!(
        DeclarationEmitter::instantiated_source_type_needs_semantic_surface(
            r#"() => { value: typeof import("./other").key; }"#
        )
    );
    assert!(
        DeclarationEmitter::instantiated_source_type_needs_semantic_surface(
            r#"() => { value: typeof import('./other').key; }"#
        )
    );
    assert!(
        !DeclarationEmitter::instantiated_source_type_needs_semantic_surface(
            r#"() => { value: string; }"#
        )
    );
}

#[test]
fn declared_call_return_refines_callback_parameter_inference_from_later_arguments() {
    let source = r#"
declare function merge<A>(left: (value: A) => void, right: (value: A) => void): (value: A) => void;
declare function acceptObject(value: Object): void;
declare function acceptString(value: string): void;
declare function acceptLiteral(value: "lit"): void;
export const first = merge(acceptObject, acceptString);
export const reversed = merge(acceptString, acceptObject);
export const literal = merge(acceptObject, acceptLiteral);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const first: (value: string) => void;"),
        "{output}"
    );
    assert!(
        output.contains("export declare const reversed: (value: string) => void;"),
        "{output}"
    );
    assert!(
        output.contains("export declare const literal: (value: \"lit\") => void;"),
        "{output}"
    );
}

#[test]
fn declared_call_return_infers_tuple_spread_parameters_from_array_literals() {
    let source = r#"
export function concat<T extends unknown[], U extends unknown[]>(t: [...T], u: [...U]): [...T, ...U] {
    return [...t, ...u];
}
export const result = concat(["x"], [1, true]);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const result: [string, number, boolean];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_preserves_bare_type_parameter_literal_argument() {
    let source = r#"
declare function valueMerge<A>(value: A, left: (value: A) => void, right: (value: A) => void): A;
declare function acceptObject(value: Object): void;
declare function acceptString(value: string): void;
const text = valueMerge("abc", acceptObject, acceptString);
const number = valueMerge(123, (value: number | string) => {}, (value: 123) => {});
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(output.contains("declare const text = \"abc\";"), "{output}");
    assert!(output.contains("declare const number = 123;"), "{output}");
}

#[test]
fn declared_call_return_uses_annotation_for_composed_literal_result() {
    let source = r#"
declare function ff2<T extends string, U extends string>(x: T, y: U): `${T}-${U}`;
const ts1 = ff2("foo", "bar");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("declare const ts1: \"foo-bar\";"),
        "{output}"
    );
    assert!(
        !output.contains("declare const ts1 = \"foo-bar\";"),
        "{output}"
    );
}

#[test]
fn declared_call_return_uses_annotation_for_single_placeholder_template_literal() {
    let source = r#"
declare function idTpl<T extends string>(x: T): `${T}`;
const a = idTpl("foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(output.contains("declare const a: \"foo\";"), "{output}");
    assert!(!output.contains("declare const a = \"foo\";"), "{output}");
}

#[test]
fn declared_call_return_uses_initializer_for_identity_type_parameter() {
    let source = r#"
declare function id<T extends string>(x: T): T;
const b = id("foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(output.contains("declare const b = \"foo\";"), "{output}");
    assert!(!output.contains("declare const b: \"foo\";"), "{output}");
}

#[test]
fn declared_call_return_uses_initializer_for_identity_with_renamed_type_parameter() {
    // The structural rule must not depend on the chosen type-parameter name.
    let source = r#"
declare function id2<X extends string>(x: X): X;
const c = id2("hello");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(output.contains("declare const c = \"hello\";"), "{output}");
    assert!(!output.contains("declare const c: \"hello\";"), "{output}");
}

#[test]
fn declared_call_return_uses_annotation_for_union_with_undefined() {
    // `T | undefined` is not an identity; the result is structurally composed
    // even when one branch happens to be a literal.
    let source = r#"
declare function maybe<T extends string>(x: T): T | undefined;
const u = maybe("foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(!output.contains("declare const u = \"foo\";"), "{output}");
}

#[test]
fn declared_call_return_uses_annotation_for_array_of_type_parameter() {
    let source = r#"
declare function wrapArr<T extends string>(x: T): T[];
const arr = wrapArr("foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(!output.contains("declare const arr = \"foo\";"), "{output}");
}

#[test]
fn declared_call_return_uses_initializer_for_second_argument_identity() {
    // The first arg is `any` so it can't supply T; T is bound from the
    // second positional argument. Return type is still bare T, so the
    // initializer form is recoverable.
    let source = r#"
declare function pickSecond<T extends string>(x: any, y: T): T;
const s = pickSecond(42, "foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(output.contains("declare const s = \"foo\";"), "{output}");
    assert!(!output.contains("declare const s: \"foo\";"), "{output}");
}

#[test]
fn declared_call_return_uses_annotation_for_object_type_return() {
    // `{ value: T }` is a composed type, not a bare type-parameter reference.
    let source = r#"
declare function unwrapBox<T extends string>(x: T): { value: T };
const wrapped = unwrapBox("foo");
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        !output.contains("declare const wrapped = \"foo\";"),
        "{output}"
    );
}

#[test]
fn returned_intrinsic_call_preserves_outer_type_parameter() {
    let source = r#"
declare function foo3<T extends string>(x: Uppercase<T>): T;
function foo4<U extends string>(x: Uppercase<U>) {
    return foo3(x);
}
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("declare function foo4<U extends string>(x: Uppercase<U>): U;"),
        "{output}"
    );
    assert!(
        !output.contains("declare function foo4<U extends string>(x: Uppercase<U>): string;"),
        "{output}"
    );
}

#[test]
fn generic_return_parameter_or_never_call_uses_nonnullable_surface() {
    let source = r#"
function error(): never {
    throw new Error();
}

function present<T>(value: T) {
    return value || error();
}
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("declare function present<T>(value: T): NonNullable<T>;"),
        "{output}"
    );
    assert!(
        !output.contains("declare function present<T>(value: T): T;"),
        "{output}"
    );
}

#[test]
fn higher_order_type_parameter_parameter_blocks_direct_literal_initializer_reuse() {
    let source = r#"
declare function direct<A>(value: A, callback: (value: A) => void): A;
declare function higher<A>(value: A, callback: (inner: (value: A) => void) => void): A;
declare function unrelated<A>(value: A, callback: (inner: (value: string) => void) => void): A;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);
    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    let find_function = |name: &str| {
        parser.arena.nodes.iter().find_map(|node| {
            let func = parser.arena.get_function(node)?;
            (emitter
                .identifier_text_from_arena(&parser.arena, func.name)
                .as_deref()
                == Some(name))
            .then_some(func)
        })
    };

    assert!(!emitter.function_has_higher_order_type_parameter_parameter(
        &parser.arena,
        find_function("direct").expect("direct function"),
        "A",
    ));
    assert!(emitter.function_has_higher_order_type_parameter_parameter(
        &parser.arena,
        find_function("higher").expect("higher function"),
        "A",
    ));
    assert!(!emitter.function_has_higher_order_type_parameter_parameter(
        &parser.arena,
        find_function("unrelated").expect("unrelated function"),
        "A",
    ));
}

#[test]
fn declared_call_return_infers_const_tuple_spread_parameters_from_array_literals() {
    let source = r#"
export function tup<T extends unknown[], U extends unknown[]>(t: [...T], u: [...U]) {
    return [1, ...t, 2, ...u, 3] as const;
}
export const result = tup(["x"], [1, true]);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output
            .contains("export declare const result: readonly [1, string, 2, number, boolean, 3];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_uses_rest_identity_source_summary_before_canonical_widening() {
    let source = r#"
declare function wide<T extends unknown[]>(...args: T): T;
declare function constrained<T extends (string | number | boolean)[]>(...args: T): T;
export const empty = wide();
export const tuple = wide(1, "x", true);
export const literalTuple = constrained(1, "x", true);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const empty: [];"),
        "{output}"
    );
    assert!(
        output.contains("export declare const tuple: [number, string, boolean];"),
        "{output}"
    );
    assert!(
        output.contains("export declare const literalTuple: [1, \"x\", true];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_infers_remaining_variadic_function_parameters() {
    let source = r#"
export function curry<T extends unknown[], U extends unknown[], R>(f: (...args: [...T, ...U]) => R, ...a: T): (...b: U) => R {
    return (...b: U) => f(...a, ...b);
}
export const fn = (a: number, b: string, c: boolean) => 0;
export const curried = curry(fn, 1);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const curried: (b: string, c: boolean) => number;"),
        "{output}"
    );
}

#[test]
fn bind_call_collapses_exhausted_local_bind_rest_tail() {
    let source = r#"
declare function bind<T, U extends unknown[], V>(f: (x: T, ...rest: U) => V, x: T): (...rest: U) => V;
declare const f20: (x: number, y: string, z: boolean) => string[];
export const f21 = bind(f20, 1);
export const f22 = bind(f21, "x");
export const f23 = bind(f22, true);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const f21: (y: string, z: boolean) => string[];"),
        "{output}"
    );
    assert!(
        output.contains("export declare const f22: (z: boolean) => string[];"),
        "{output}"
    );
    assert!(
        output.contains("export declare const f23: () => string[];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_preserves_remaining_variadic_rest_parameter() {
    let source = r#"
export function curry<T extends unknown[], U extends unknown[], R>(f: (...args: [...T, ...U]) => R, ...a: T): (...b: U) => R {
    return (...b: U) => f(...a, ...b);
}
export const fn = (x: number, ready: boolean, ...args: string[]) => 0;
export const curried = curry(fn, 1);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains(
            "export declare const curried: (ready: boolean, ...args: string[]) => number;"
        ),
        "{output}"
    );
}

#[test]
fn declared_call_return_infers_unannotated_variadic_curry_return_type() {
    let source = r#"
export function curry<T extends unknown[], U extends unknown[], R>(f: (...args: [...T, ...U]) => R, ...a: T) {
    return (...b: U) => f(...a, ...b);
}
export const fn = (a: number, b: string, c: boolean) => 0;
export const curried = curry(fn, 1);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const curried: (b: string, c: boolean) => number;"),
        "{output}"
    );
}

#[test]
fn declaration_emit_preserves_variadic_parameter_call_return_type() {
    let source = r#"
export function invoke<T extends unknown[], U extends unknown[], R>(f: (...args: [...T, ...U]) => R, t: [...T], u: [...U]) {
    return f(...t, ...u);
}
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains(
            "export declare function invoke<T extends unknown[], U extends unknown[], R>(f: (...args: [...T, ...U]) => R, t: [...T], u: [...U]): R;"
        ),
        "{output}"
    );
}

#[test]
fn declaration_emit_summarizes_spread_array_parameter_return_type() {
    let source = r#"
	export function concat<T extends readonly unknown[], U extends readonly unknown[]>(t: T, u: U) {
	    return [...t, ...u];
}
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains(
            "export declare function concat<T extends readonly unknown[], U extends readonly unknown[]>(t: T, u: U): (T[number] | U[number])[];"
        ),
        "{output}"
    );
}

#[test]
fn declared_call_return_orders_numeric_tuple_index_union_like_tsc() {
    let source = r#"
export function concat<A extends readonly unknown[], B extends readonly unknown[]>(a: A, b: B) {
    return [...a, ...b];
}
export const result = concat([1, 2, 3] as const, [4, 5, 6] as const);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("export declare const result: (1 | 2 | 3 | 4 | 5 | 6)[];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_infers_arrayified_variadic_tuple_rest() {
    let source = r#"
type Arrayify<T> = { [P in keyof T]: T[P][] };
declare function fm<T extends unknown[]>(t: Arrayify<[string, number, ...T]>): T;
let value = fm([["abc"], [42], [true], ["def"]]);
"#;
    let output = emit_test_dts_with_binding(source);

    assert!(
        output.contains("declare let value: [boolean, string];"),
        "{output}"
    );
}

#[test]
fn declared_call_return_inverts_structural_partial_like_mapped_alias() {
    let source = r#"
type OptionalShape<T> = { [Key in keyof T]?: T[Key] };
declare function complete<T>(x: OptionalShape<T>): T;
declare let value: { a?: number; b: string | undefined };
let y = complete(value);
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
        Some("{\n    a: number;\n    b: string;\n}".to_string())
    );
}

#[test]
fn partial_required_call_surface_can_replace_preferred_optional_undefined_object() {
    let source = r#"
type OptionalShape<Shape> = { [Field in keyof Shape]?: Shape[Field] };
declare function complete<Shape>(x: OptionalShape<Shape>): Shape;
declare let value: { a: number | undefined, b?: string[] };
let y = complete(value);
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

    assert!(
        emitter.partial_required_call_reused_type_should_replace_preferred(
            call_idx,
            "{\n    a: number;\n    b: string[];\n}",
            "{\n    a: number | undefined;\n    b: string[];\n}",
        )
    );
}

#[test]
fn call_reused_type_inverts_reverse_mapped_handler_parameters() {
    let source = r#"
type Callbacks<Shape> = { [Field in keyof Shape]: (value: Shape[Field]) => void };
declare function listen<Shape>(handlers: Callbacks<Shape>): Shape;
let result = listen({ ready: () => {}, flag: (value: boolean) => {} });
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
        emitter.call_expression_reused_type_text(call_idx),
        Some("{\n    ready: unknown;\n    flag: boolean;\n}".to_string())
    );
}

#[test]
fn declared_call_return_does_not_treat_shadowed_partial_name_as_builtin() {
    let source = r#"
type Partial<T> = T;
declare function complete<T>(x: Partial<T>): T;
declare let value: { a?: number };
let y = complete(value);
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
        None
    );
}

#[test]
fn exported_call_preserves_inexact_optional_mapped_intersection_surface() {
    let output = emit_test_dts_with_binding(
        r#"
type InexactOptionals<A> = {
    [K in keyof A as undefined extends A[K] ? K : never]?: undefined extends A[K]
    ? A[K] | undefined
    : A[K];
} & {
    [K in keyof A as undefined extends A[K] ? never : K]: A[K];
};

type In = {
    foo?: string;
    bar: number;
    baz: undefined;
}

type Out = InexactOptionals<In>

const foo = <A = {}>() => (x: Out & A) => null

export const baddts = foo()
"#,
    );

    assert_eq!(
        output.trim(),
        "export declare const baddts: (x: {\n    foo?: string | undefined;\n    baz?: undefined;\n} & {\n    bar: number;\n}) => any;"
    );
}

#[test]
fn exported_call_preserves_renamed_inexact_optional_mapped_intersection_surface() {
    let output = emit_test_dts_with_binding(
        r#"
type Loose<T> = {
    [Key in keyof T as undefined extends T[Key] ? Key : never]?: undefined extends T[Key]
    ? T[Key] | undefined
    : T[Key];
} & {
    [Key in keyof T as undefined extends T[Key] ? never : Key]: T[Key];
};

type Input = {
    maybe?: string;
    count: number;
    nil: undefined;
}

type Result = Loose<Input>

const make = <Extra = {}>() => (value: Result & Extra) => null

export const publicValue = make()
"#,
    );

    assert_eq!(
        output.trim(),
        "export declare const publicValue: (value: {\n    maybe?: string | undefined;\n    nil?: undefined;\n} & {\n    count: number;\n}) => any;"
    );
}

// A bare identifier that widens to a function/class/enum/namespace symbol can be
// spelled `typeof Name` in a `.d.ts` only when that name is visible at the emit
// position. `symbol_declaration_reachable_from_module_scope` is the scope half
// of that visibility question: a declaration nested inside a function/block body
// is a local whose name is not emitted, so `typeof` must not be used and the
// structural shape is expanded instead. These asserts pin the predicate directly
// so the `declFileTypeofFunction` / mixin-local-class family cannot silently
// re-widen to `typeof <local>`.
fn symbol_reachable_from_module_scope(source: &str, escaped_name: &str, flag: u32) -> bool {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    let interner = TypeInterner::new();
    let type_cache = crate::type_cache_view::TypeCacheView::default();
    let emitter = DeclarationEmitter::with_type_info(&parser.arena, type_cache, &interner, &binder);

    let symbol = binder
        .symbols
        .iter()
        .find(|sym| sym.escaped_name == escaped_name && (sym.flags & flag) != 0)
        .unwrap_or_else(|| panic!("no `{escaped_name}` symbol with the requested flag"));
    emitter.symbol_declaration_reachable_from_module_scope(symbol)
}

#[test]
fn module_level_function_and_class_are_reachable_from_module_scope() {
    let source = r#"
function topFn() { return topFn; }
class TopClass {}
enum TopEnum { A }
"#;
    assert!(symbol_reachable_from_module_scope(
        source,
        "topFn",
        tsz_binder::symbol_flags::FUNCTION,
    ));
    assert!(symbol_reachable_from_module_scope(
        source,
        "TopClass",
        tsz_binder::symbol_flags::CLASS,
    ));
    assert!(symbol_reachable_from_module_scope(
        source,
        "TopEnum",
        tsz_binder::symbol_flags::ENUM,
    ));
}

#[test]
fn namespace_member_function_is_reachable_from_module_scope() {
    // A `MODULE_BLOCK` is not a value-scope boundary, so namespace members stay
    // reachable — `typeof N.f` remains spellable.
    let source = r#"
namespace N {
    export function nested() { return nested; }
}
"#;
    assert!(symbol_reachable_from_module_scope(
        source,
        "nested",
        tsz_binder::symbol_flags::FUNCTION,
    ));
}

#[test]
fn function_local_declarations_are_not_reachable_from_module_scope() {
    // `bar` (block body), `LocalClass` (block body), and `ArrowLocal` (expression
    // body) are all trapped below module scope; none may print as `typeof`.
    let source = r#"
function foo5(x: number) {
    function bar(y: number) { return y; }
    return bar;
}
function mk() {
    class LocalClass {}
    return LocalClass;
}
const arrow = () => class ArrowLocal {};
"#;
    assert!(!symbol_reachable_from_module_scope(
        source,
        "bar",
        tsz_binder::symbol_flags::FUNCTION,
    ));
    assert!(!symbol_reachable_from_module_scope(
        source,
        "LocalClass",
        tsz_binder::symbol_flags::CLASS,
    ));
    assert!(!symbol_reachable_from_module_scope(
        source,
        "ArrowLocal",
        tsz_binder::symbol_flags::CLASS,
    ));
}
