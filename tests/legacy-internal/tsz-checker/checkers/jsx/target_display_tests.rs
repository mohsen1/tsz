//! JSX target-display unit tests.

use crate::test_utils::check_source;

fn check_jsx(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;

    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

/// JSX TS2322 target display preserves `| undefined` for optional props
/// from anonymous inline `IntrinsicElements` types when the attribute is a
/// bare string literal (no `{}`). tsc shows `boolean | undefined`, not
/// just `boolean`, in that case.
#[test]
fn jsx_bare_string_literal_attr_to_optional_inline_prop_displays_undefined() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { n?: boolean };
    }
}
<test1 n='true' />;
"#;
    let diagnostics = check_jsx(source);
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

/// JSX TS2322 target display strips `| undefined` for optional props from
/// NAMED interface types, even with bare string literal initializers.
/// tsc shows just `number`, not `number | undefined`, for `Attribs1.x?: number`.
#[test]
fn jsx_bare_string_literal_attr_to_optional_named_prop_strips_undefined() {
    let source = r#"
interface Attribs1 { x?: number }
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
<test1 x="32" />;
"#;
    let diagnostics = check_jsx(source);
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

/// JSX expression initializers (with `{...}`) for optional inline props use
/// the standard display path that strips `| undefined`, matching tsc.
#[test]
fn jsx_expression_attr_to_optional_inline_prop_strips_undefined() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        div: { text?: string; width?: number };
    }
}
<div width={'foo'} />;
"#;
    let diagnostics = check_jsx(source);
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
