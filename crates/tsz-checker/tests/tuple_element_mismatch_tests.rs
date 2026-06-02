//! Tuple element type-mismatch diagnostic elaboration must match `tsc`.
//!
//! `tsc` keys the elaboration shape on the tuple's arity:
//! - **Multi-element tuples** disambiguate the failing slot with TS2626
//!   `Type at position N in source is not compatible with type at position N in
//!   target.`, nested beneath the outer `Type 'S' is not assignable to type
//!   'T'.` line, then the inner element failure.
//! - **Single-element tuples** have no position to disambiguate, so `tsc` omits
//!   the positional line and relates the element types directly with the
//!   standard `Type 'se' is not assignable to type 'te'.` message, recursing
//!   into the element's own failure.
//!
//! These assertions are pinned to the exact `tsc` 5.8 output.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322(source: &str) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS2322 diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related(diagnostic)
        .iter()
        .any(|message| message == expected)
}

#[test]
fn tuple_second_element_mismatch_reports_position_1() {
    let diagnostic = ts2322(
        r#"
declare let y: [string, string];
let x: [string, number] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(
            &diagnostic,
            "Type at position 1 in source is not compatible with type at position 1 in target."
        ),
        "missing TS2626 position elaboration; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing inner element failure; related = {messages:#?}"
    );
}

#[test]
fn tuple_first_element_mismatch_reports_position_0() {
    let diagnostic = ts2322(
        r#"
declare let y: [boolean, string];
let x: [string, string] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(
            &diagnostic,
            "Type at position 0 in source is not compatible with type at position 0 in target."
        ),
        "missing TS2626 position elaboration; related = {messages:#?}"
    );
}

fn has_position_line(diagnostic: &Diagnostic) -> bool {
    related(diagnostic)
        .iter()
        .any(|message| message.contains("in source is not compatible with type at position"))
}

#[test]
fn single_element_tuple_object_property_mismatch_omits_position_line() {
    // Single-element tuple: tsc relates the element types directly
    // (`Type '{ a: string; }' is not assignable to type '{ a: number; }'.`)
    // and never emits the TS2626 positional line.
    let diagnostic = ts2322(
        r#"
declare let y: [{ a: string }];
let x: [{ a: number }] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        !has_position_line(&diagnostic),
        "single-element tuple must not emit the TS2626 positional line; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '{ a: string; }' is not assignable to type '{ a: number; }'."
        ),
        "missing element-type relation header; related = {messages:#?}"
    );
    assert!(
        has_related(&diagnostic, "Types of property 'a' are incompatible."),
        "missing nested property elaboration; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure; related = {messages:#?}"
    );
}

#[test]
fn nested_single_element_tuple_mismatch_relates_each_level_without_position() {
    // tsc chain (no positional lines):
    //   Type '[[string]]' is not assignable to type '[[number]]'.
    //     Type '[string]' is not assignable to type '[number]'.
    //       Type 'string' is not assignable to type 'number'.
    let diagnostic = ts2322(
        r#"
declare let y: [[string]];
let x: [[number]] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        !has_position_line(&diagnostic),
        "single-element tuples must not emit positional lines; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '[string]' is not assignable to type '[number]'."
        ),
        "missing inner tuple relation level; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure; related = {messages:#?}"
    );
}

#[test]
fn deeply_nested_single_element_tuple_relates_every_level() {
    // `[[[string]]]` vs `[[[number]]]` must relate all three tuple levels and
    // never emit a positional line (tsc parity).
    let diagnostic = ts2322(
        r#"
declare let y: [[[string]]];
let x: [[[number]]] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        !has_position_line(&diagnostic),
        "single-element tuples must not emit positional lines; related = {messages:#?}"
    );
    for level in [
        "Type '[[string]]' is not assignable to type '[[number]]'.",
        "Type '[string]' is not assignable to type '[number]'.",
        "Type 'string' is not assignable to type 'number'.",
    ] {
        assert!(
            has_related(&diagnostic, level),
            "missing tuple relation level {level:?}; related = {messages:#?}"
        );
    }
}

#[test]
fn single_element_tuple_nested_in_multi_element_tuple_keeps_position_and_header() {
    // The outer tuple is multi-element (positional line warranted); its failing
    // element is a single-element tuple, so tsc shows the positional line, then
    // the element-type header, then the leaf:
    //   Type at position 0 in source is not compatible with ... position 0 ...
    //     Type '[string]' is not assignable to type '[number]'.
    //       Type 'string' is not assignable to type 'number'.
    let diagnostic = ts2322(
        r#"
declare let y: [[string], boolean];
let x: [[number], boolean] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(
            &diagnostic,
            "Type at position 0 in source is not compatible with type at position 0 in target."
        ),
        "outer multi-element tuple must keep its positional line; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '[string]' is not assignable to type '[number]'."
        ),
        "missing single-element tuple header under the positional line; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure; related = {messages:#?}"
    );
}

#[test]
fn single_element_tuple_union_element_does_not_duplicate_header() {
    // The element relation `string | boolean` -> `number` self-heads with its
    // own `Type 'string | boolean' …'number'.` line, so the renderer must
    // delegate to it rather than emit a second copy of that header. tsc:
    //   Type '[string | boolean]' is not assignable to type '[number]'.
    //     Type 'string | boolean' is not assignable to type 'number'.
    //       Type 'string' is not assignable to type 'number'.
    let diagnostic = ts2322(
        r#"
declare let y: [string | boolean];
let x: [number] = y;
"#,
    );
    let messages = related(&diagnostic);
    assert!(
        !has_position_line(&diagnostic),
        "single-element tuple must not emit positional lines; related = {messages:#?}"
    );
    let union_header_count = messages
        .iter()
        .filter(|message| {
            message.as_str() == "Type 'string | boolean' is not assignable to type 'number'."
        })
        .count();
    assert_eq!(
        union_header_count, 1,
        "union element header must appear exactly once (no duplication); related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure; related = {messages:#?}"
    );
}
