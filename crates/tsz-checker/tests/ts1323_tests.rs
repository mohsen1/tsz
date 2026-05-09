//! Tests for TS1323: Dynamic import module flag validation.
//!
//! TSC emits TS1323 when `import()` is used but the `--module` flag is set to
//! a value that doesn't support dynamic imports (e.g., `es2015` or `none`).
//! Supported modules: es2020, es2022, esnext, commonjs, amd, system, umd,
//! node16, nodenext, preserve.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn get_diagnostics_with_module_and_file(
    source: &str,
    file_name: &str,
    module: ModuleKind,
) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        file_name,
        CheckerOptions {
            module,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn get_diagnostics_with_module(source: &str, module: ModuleKind) -> Vec<(u32, String)> {
    get_diagnostics_with_module_and_file(source, "test.ts", module)
}

fn get_js_diagnostics_with_module(source: &str, module: ModuleKind) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            module,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn has_ts1323(source: &str, module: ModuleKind) -> bool {
    get_diagnostics_with_module(source, module)
        .iter()
        .any(|d| d.0 == 1323)
}

const DYNAMIC_IMPORT: &str = r#"import("./foo");"#;

#[test]
fn dynamic_import_emits_ts1323_for_es2015() {
    assert!(has_ts1323(DYNAMIC_IMPORT, ModuleKind::ES2015));
}

#[test]
fn dynamic_import_emits_ts1323_for_none() {
    assert!(has_ts1323(DYNAMIC_IMPORT, ModuleKind::None));
}

#[test]
fn dynamic_import_no_ts1323_for_es2020() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::ES2020));
}

#[test]
fn dynamic_import_no_ts1323_for_esnext() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::ESNext));
}

#[test]
fn dynamic_import_no_ts1323_for_commonjs() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::CommonJS));
}

#[test]
fn dynamic_import_no_ts1323_for_nodenext() {
    assert!(!has_ts1323(DYNAMIC_IMPORT, ModuleKind::NodeNext));
}

#[test]
fn message_text_matches_tsc() {
    let diags = get_diagnostics_with_module(DYNAMIC_IMPORT, ModuleKind::ES2015);
    let msg = &diags.iter().find(|d| d.0 == 1323).unwrap().1;
    assert!(
        msg.contains("'--module'"),
        "Expected '--module' in message, got: {msg}"
    );
    assert!(
        msg.contains("'es2020'"),
        "Expected 'es2020' in message, got: {msg}"
    );
}

#[test]
fn import_type_query_member_access_does_not_emit_ts1323_for_es2015() {
    let source = r#"
export declare class A {
    static foo(): void;
}

export const foo: typeof import("./a").A.foo;
"#;

    let diags = get_diagnostics_with_module_and_file(source, "index.d.ts", ModuleKind::ES2015);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1323),
        "Did not expect TS1323 for typeof import(\"./a\").A.foo in a declaration file, got: {diags:?}"
    );
}

#[test]
fn import_type_member_access_does_not_emit_ts1323_for_es2015() {
    let source = r#"
type Thing = import("./mod").Thing;
type Value = typeof import("./mod").Thing;
"#;

    let diags = get_diagnostics_with_module(source, ModuleKind::ES2015);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1323),
        "Did not expect TS1323 for import(\"./mod\") in type syntax, got: {diags:?}"
    );
}

#[test]
fn jsdoc_import_type_member_access_does_not_emit_ts1323_for_es2015() {
    let source = r#"
/** @param {import("./mod").Thing} x
 *  @param {typeof import("./mod").Thing} y */
function f(x, y) {
    return x || y;
}
"#;

    let diags = get_js_diagnostics_with_module(source, ModuleKind::ES2015);
    assert!(
        !diags.iter().any(|(code, _)| *code == 1323),
        "Did not expect TS1323 for JSDoc import(\"./mod\") type syntax, got: {diags:?}"
    );
}
