//! JSX optional prop diagnostic display tests.

use crate::test_utils::check_source;

fn check_jsx(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;

    check_source(
        source,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn jsx_bare_string_literal_attr_to_optional_inline_prop_displays_undefined() {
    let diagnostics = check_jsx(
        r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { n?: boolean };
    }
}
<test1 n='true' />;
"#,
    );
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for string assigned to optional boolean prop");
    assert!(
        ts2322
            .message_text
            .contains("is not assignable to type 'boolean | undefined'"),
        "TS2322 target should display `boolean | undefined` for bare string literal \
         to optional inline prop; got: {}",
        ts2322.message_text
    );
}

#[test]
fn jsx_bare_string_literal_attr_to_optional_named_prop_strips_undefined() {
    let diagnostics = check_jsx(
        r#"
interface Attribs1 { x?: number }
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
<test1 x="32" />;
"#,
    );
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for string assigned to optional number prop");
    assert!(
        ts2322
            .message_text
            .contains("is not assignable to type 'number'")
            && !ts2322.message_text.contains("number | undefined"),
        "TS2322 target should display `number` (no `| undefined`) for bare string \
         literal to optional NAMED prop; got: {}",
        ts2322.message_text
    );
}

#[test]
fn jsx_expression_attr_to_optional_inline_prop_strips_undefined() {
    let diagnostics = check_jsx(
        r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: { text?: string; width?: number };
    }
}
<div width={'foo'} />;
"#,
    );
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for string in JSX expression to optional number prop");
    assert!(
        ts2322
            .message_text
            .contains("is not assignable to type 'number'")
            && !ts2322.message_text.contains("number | undefined"),
        "TS2322 target should display `number` (no `| undefined`) for JSX \
         expression initializer; got: {}",
        ts2322.message_text
    );
}
