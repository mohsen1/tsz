//! JSX spread assignability regression tests.

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

#[test]
fn jsx_spread_attributes_resolution12_reports_merged_effective_source_once() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { div: {}; }
}
interface Prop { x: 2; y: false; overwrite: string; }
declare class Comp { props: Prop; }
const obj = {};
const obj1: { x: 2 } = { x: 2 };
let anyobj: any;
let x = <Comp {...obj} y overwrite="hi" {...obj1} />;
let x1 = <Comp overwrite="hi" {...obj1} x={3} {...{ y: true }} />;
let x2 = <Comp {...anyobj} x={3} />;
let x3 = <Comp overwrite="hi" {...obj1} {...{ y: true }} />;
"#;
    let diagnostics = check_jsx(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected exactly the three tsc TS2322 diagnostics for x, x1, and x3; got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|d| d
            .message_text
            .contains("Type 'true' is not assignable to type 'false'.")),
        "Expected shorthand y mismatch for x; got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|d| d
            .message_text
            .contains("Type '3' is not assignable to type '2'.")),
        "Expected explicit x mismatch for x1; got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|d| d.message_text.contains(
            "Type '{ y: true; x: 2; overwrite: string; }' is not assignable to type 'Prop'."
        )),
        "Expected merged effective attribute display for x3; got: {diagnostics:?}"
    );
    assert!(
        !ts2322.iter().any(|d| d
            .message_text
            .contains("Type '{ y: true; }' is not assignable to type 'Prop'.")),
        "Spread-only display should not be emitted for this fixture; got: {diagnostics:?}"
    );
}
