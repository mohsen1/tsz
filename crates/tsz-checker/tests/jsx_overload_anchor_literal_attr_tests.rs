//! Locks tsc-parity for the JSX overload TS2769 anchor when one overload
//! accepts a syntactic literal attribute value. Without literal-type
//! preservation in `collect_jsx_provided_attrs`, the per-overload
//! assignability walk in `jsx_overload_explicit_failure_attr` produces
//! false-positive failure attrs and skews the shared-anchor heuristic.
//!
//! Regression target: `contextuallyTypedStringLiteralsInJsxAttributes02.tsx`
//! (b4 case — `<MainButton goTo="home" extra />`).

use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;

const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

fn jsx_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.tsx",
        CheckerOptions {
            jsx_mode: JsxMode::Preserve,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.start, d.message_text))
    .collect()
}

#[test]
fn ts2769_anchors_at_tag_when_literal_attr_succeeds_one_overload() {
    // The b4 case from contextuallyTypedStringLiteralsInJsxAttributes02.tsx:
    // overload 1 (ButtonProps) has no `goTo`, overload 2 (LinkProps) has
    // `goTo: "home" | "contact"`. With literal-type preservation, the
    // syntactic `goTo="home"` succeeds against overload 2 (literal match)
    // and fails only on excess `extra`. Overload 1 fails on `goTo`.
    // Different failure attrs → fall through to tag-name anchoring.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface ButtonProps {{ onClick: (k: "left" | "right") => void; }}
interface LinkProps {{ goTo: "home" | "contact"; }}
declare function MainButton(buttonProps: ButtonProps): JSX.Element;
declare function MainButton(linkProps: LinkProps): JSX.Element;
const b4 = <MainButton goTo="home" extra />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2769: Vec<&(u32, u32, String)> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .collect();
    assert!(
        !ts2769.is_empty(),
        "Expected at least one TS2769. Got: {diags:?}"
    );
    // Determine the byte offset of `MainButton` (the JSX tag identifier
    // after the `<`) — that's where tsc anchors when overloads disagree
    // on which attribute fails.
    let main_button_open = source
        .rfind("<MainButton")
        .expect("repro must contain `<MainButton goTo=\"home\" extra />`");
    let tag_start = main_button_open + "<".len();
    // Find the `goTo` attribute name position too — that's where tsz
    // used to (incorrectly) anchor before the fix.
    let go_to_pos = source.rfind("goTo=").expect("repro must contain goTo=");

    for (_, start, _) in &ts2769 {
        assert!(
            *start as usize == tag_start,
            "TS2769 must anchor at the `MainButton` tag (offset {tag_start}), not the `goTo` attribute (offset {go_to_pos}). Got start={start}."
        );
    }
}

#[test]
fn sfc_overload_body_children_reject_overload_without_children_prop() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace JSX {{ interface Element {{ __jsxElementBrand: never; }} }}
interface TestingOptionalComponent {{
    (a: {{ y1?: string; y2?: number }}): JSX.Element;
    (a: {{ y1?: string; y2?: number; children: JSX.Element }}): JSX.Element;
    (a: {{ y1: boolean; y2?: number; y3: boolean }}): JSX.Element;
}}
declare const TestingOptional: TestingOptionalComponent;
const e4 = <TestingOptional y1="hello" y2={{1000}}>Hi</TestingOptional>;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2769: Vec<&(u32, u32, String)> = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL)
        .collect();
    assert_eq!(
        ts2769.len(),
        1,
        "Body text should reject every SFC overload and emit one TS2769. Got: {diags:?}"
    );

    let tag_start = source
        .rfind("<TestingOptional")
        .expect("repro must contain `<TestingOptional`")
        + "<".len();
    let (_, start, _) = ts2769[0];
    assert_eq!(
        *start as usize, tag_start,
        "TS2769 should anchor at the JSX tag when synthesized children participate in SFC overload resolution"
    );
}
