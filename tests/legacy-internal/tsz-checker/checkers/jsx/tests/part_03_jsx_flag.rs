//! Tests for TS17004 ("Cannot use JSX unless the '--jsx' flag is provided").
//!
//! tsz must emit TS17004 once per JSX opening element, self-closing element,
//! and fragment when `compilerOptions.jsx` is unset (`JsxMode::None`), matching
//! tsc's `checkJsxPreconditions`. When any `jsx` mode is set, the diagnostic
//! must not appear.

use super::*;
use crate::context::CheckerOptions;
use tsz_common::checker_options::JsxMode;

const TS17004: u32 = 17004;

fn jsx_codes_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<u32> {
    let opts = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts17004_emitted_for_element_when_jsx_flag_absent() {
    let codes = jsx_codes_with_mode("const a = <div />;\n", JsxMode::None);
    assert_eq!(
        codes.iter().filter(|&&c| c == TS17004).count(),
        1,
        "expected exactly one TS17004 for a self-closing element, got {codes:?}"
    );
}

#[test]
fn ts17004_emitted_once_per_opening_self_closing_and_fragment() {
    // Two opening/self-closing elements (`div`, `span`) plus one fragment.
    let source = "const a = <div></div>;\nconst b = <span />;\nconst c = <></>;\n";
    let codes = jsx_codes_with_mode(source, JsxMode::None);
    assert_eq!(
        codes.iter().filter(|&&c| c == TS17004).count(),
        3,
        "expected one TS17004 per opening-like element and fragment, got {codes:?}"
    );
}

#[test]
fn ts17004_counts_nested_elements() {
    // Outer `div`, inner `span`, and a nested fragment => three JSX nodes.
    let source = "const t = <div><span /><></></div>;\n";
    let codes = jsx_codes_with_mode(source, JsxMode::None);
    assert_eq!(
        codes.iter().filter(|&&c| c == TS17004).count(),
        3,
        "expected TS17004 for each nested JSX node, got {codes:?}"
    );
}

#[test]
fn ts17004_absent_when_jsx_preserve() {
    let codes = jsx_codes_with_mode("const a = <div />;\n", JsxMode::Preserve);
    assert!(
        !codes.contains(&TS17004),
        "TS17004 must not fire when jsx=preserve, got {codes:?}"
    );
}

#[test]
fn ts17004_absent_when_jsx_react() {
    let codes = jsx_codes_with_mode("const a = <div />;\n", JsxMode::React);
    assert!(
        !codes.contains(&TS17004),
        "TS17004 must not fire when jsx=react, got {codes:?}"
    );
}

#[test]
fn ts17004_absent_when_jsx_react_jsx() {
    let codes = jsx_codes_with_mode("const a = <div />;\n", JsxMode::ReactJsx);
    assert!(
        !codes.contains(&TS17004),
        "TS17004 must not fire when jsx=react-jsx, got {codes:?}"
    );
}
