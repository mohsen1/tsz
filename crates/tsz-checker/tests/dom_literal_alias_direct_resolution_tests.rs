//! DOM literal-union aliases should resolve directly without losing their
//! assignment behavior or alias display.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_lib_files};

fn check_with_dom(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts", "dom.d.ts"]);
    assert_eq!(libs.len(), 2, "DOM literal alias tests require es5 and dom");
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

#[test]
fn dom_literal_alias_accepts_declared_literal() {
    let diagnostics = check_with_dom(
        r#"
const ready: DocumentReadyState = "complete";
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "DocumentReadyState must accept one of its declared literals, got: {diagnostics:#?}"
    );
}

#[test]
fn dom_literal_alias_rejects_unknown_literal_with_alias_display() {
    let diagnostics = check_with_dom(
        r#"
const ready: DocumentReadyState = "settled";
"#,
    );

    assert_eq!(
        diagnostics.len(),
        1,
        "unknown DocumentReadyState literal should produce one diagnostic, got: {diagnostics:#?}"
    );
    let (code, message) = &diagnostics[0];
    assert_eq!(*code, 2322, "expected TS2322, got: {diagnostics:#?}");
    assert!(
        message.contains("Type '\"settled\"' is not assignable to type 'DocumentReadyState'."),
        "diagnostic should preserve the DOM alias target display, got: {message:?}"
    );
}

#[test]
fn local_dom_literal_alias_shadow_stays_local() {
    let diagnostics = check_with_dom(
        r#"
export {};
type DocumentReadyState = "idle";
const ready: DocumentReadyState = "complete";
"#,
    );

    assert_eq!(
        diagnostics.len(),
        1,
        "local alias shadow should reject only the non-local literal, got: {diagnostics:#?}"
    );
    let (code, message) = &diagnostics[0];
    assert_eq!(*code, 2322, "expected TS2322, got: {diagnostics:#?}");
    assert!(
        message.contains("Type '\"complete\"' is not assignable to type '\"idle\"'."),
        "local alias shadow must not reuse the global DOM alias, got: {message:?}"
    );
}
