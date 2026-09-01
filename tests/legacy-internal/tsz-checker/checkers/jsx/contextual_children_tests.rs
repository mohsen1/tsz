//! JSX contextual children regression tests.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source;

fn check_jsx_full_strict(source: &str) -> Vec<Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        strict_function_types: true,
        strict_bind_call_apply: true,
        strict_property_initialization: true,
        no_implicit_this: true,
        always_strict: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

#[test]
fn jsx_zero_param_child_callback_with_mismatched_literal_return_widens_numeric_display() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { [key: string]: Element }
}
interface LitProps<T> { prop: T, children: (x: this) => T }
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const mismatched = <ElemLit prop="x">{() => 12}</ElemLit>;
"#;
    let diagnostics = check_jsx_full_strict(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for `() => 12` against contextual return `\"x\"`");
    assert_eq!(
        diag.start,
        source.find("12").expect("fixture contains `12`") as u32,
        "TS2322 should anchor at the arrow body literal, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("Type 'number' is not assignable to type '\"x\"'.")
            && !diag.message_text.contains("Type '12'"),
        "TS2322 should widen numeric arrow-body literals for display, got: {diag:?}"
    );
}

#[test]
fn jsx_children_attribute_callback_with_mismatched_literal_return_elaborates_at_body() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { [key: string]: Element }
}
interface LitProps<T> { prop: T, children: (x: this) => T }
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
ElemLit({prop: "x", children: () => "x"});
const j = <ElemLit prop="x" children={() => "x"} />;
const jj = <ElemLit prop="x">{() => "x"}</ElemLit>;
const arg = <ElemLit prop="x" children={p => "y"} />;
"#;
    let diagnostics = check_jsx_full_strict(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for `p => \"y\"` against contextual return `\"x\"`");
    assert_eq!(
        diag.start,
        source.find("\"y\"").expect("fixture contains `\"y\"`") as u32,
        "TS2322 should anchor at the arrow body string literal, got: {diag:?}"
    );
    assert!(
        diag.message_text
            .contains("Type '\"y\"' is not assignable to type '\"x\"'.")
            && !diag.message_text.contains("=>"),
        "TS2322 should elaborate to the body-level literal mismatch, got: {diag:?}"
    );
}
