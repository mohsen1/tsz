use super::*;

fn jsdoc_import_specifiers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_jsdoc_import_call_specifiers(text, &mut out);
    out.into_iter()
        .map(|(specifier, _mode)| specifier)
        .collect()
}

#[test]
fn push_finds_single_and_double_quoted_import_calls() {
    assert_eq!(
        jsdoc_import_specifiers("import('@scope/pkg')"),
        ["@scope/pkg"]
    );
    assert_eq!(jsdoc_import_specifiers("import(\"pkg\")"), ["pkg"]);
    assert_eq!(jsdoc_import_specifiers("import('./rel')"), ["./rel"]);
}

#[test]
fn push_tolerates_whitespace_before_specifier() {
    assert_eq!(jsdoc_import_specifiers("import(  'pkg' )"), ["pkg"]);
}

#[test]
fn push_finds_multiple_import_calls_in_one_comment() {
    let text = "@type {(a: import('a').A, b: import(\"b\").B) => import('c').C}";
    assert_eq!(jsdoc_import_specifiers(text), ["a", "b", "c"]);
}

#[test]
fn push_requires_word_boundary_before_import() {
    // `reimport(` must not be treated as an `import(` call.
    assert!(jsdoc_import_specifiers("reimport('pkg')").is_empty());
}

#[test]
fn push_ignores_unquoted_and_unterminated() {
    assert!(jsdoc_import_specifiers("import(foo)").is_empty());
    assert!(jsdoc_import_specifiers("import('unterminated").is_empty());
}

#[test]
fn push_parses_inline_import_resolution_mode_attribute() {
    use tsz::module_resolver::ImportingModuleKind;
    let collect = |text: &str| {
        let mut out = Vec::new();
        push_jsdoc_import_call_specifiers(text, &mut out);
        out
    };
    assert_eq!(
        collect(r#"import("pkg", { with: { "resolution-mode": "import" } }).Foo"#),
        [("pkg".to_string(), Some(ImportingModuleKind::Esm))]
    );
    assert_eq!(
        collect(r#"import('pkg', { with: { 'resolution-mode': 'require' } }).Foo"#),
        [("pkg".to_string(), Some(ImportingModuleKind::CommonJs))]
    );
    // A bare inline import type query carries no override.
    assert_eq!(collect(r#"import("pkg").Foo"#), [("pkg".to_string(), None)]);
}

#[test]
fn text_collection_finds_jsdoc_typedef_import_type() {
    let path = Path::new("test.js");
    let text = r#"
/** @typedef {import('@lion/ajax').LionRequestInit} LionRequestInit */
/** @type {LionRequestInit} */
let v;
"#;
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"@lion/ajax".to_string()),
        "JSDoc @typedef import-type specifier should be collected, got: {specifiers:?}"
    );
}

#[test]
fn text_collection_finds_jsdoc_param_and_type_import_types() {
    let path = Path::new("test.js");
    let text = r#"
/** @param {import('pkg-a').A} a */
function f(a) {}
/** @type {(x: import('pkg-b').B) => void} */
let g;
"#;
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"pkg-a".to_string()),
        "got: {specifiers:?}"
    );
    assert!(
        specifiers.contains(&"pkg-b".to_string()),
        "got: {specifiers:?}"
    );
}

#[test]
fn text_collection_finds_jsdoc_import_type_without_other_imports() {
    // Regression: a file whose ONLY module reference is a JSDoc import-type
    // (no code import, no `@import` tag) must still bypass the token-only
    // fast path and collect the specifier.
    let path = Path::new("test.js");
    let text = "/** @type {import('only-jsdoc').T} */\nlet v;\n";
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        specifiers.contains(&"only-jsdoc".to_string()),
        "JSDoc-only import-type must be collected via full parse, got: {specifiers:?}"
    );
}

#[test]
fn text_collection_ignores_import_call_outside_jsdoc_comment() {
    // `import('x')` appearing only inside a string literal in code is not a
    // JSDoc reference; the comment-scan must not pick it up. (The AST/token
    // path owns real dynamic imports.)
    let path = Path::new("test.js");
    let text = "const s = \"import('not-a-module')\";\n";
    let specifiers = collect_module_specifiers_from_text(path, text);
    assert!(
        !specifiers.contains(&"not-a-module".to_string()),
        "string-literal text must not be collected as a JSDoc import, got: {specifiers:?}"
    );
}
