//! Tuple element type-mismatch diagnostics must carry the element position.
//!
//! tsc treats a failing tuple element specially: the outer TS2322
//! `Type 'S' is not assignable to type 'T'.` line is followed by TS2626
//! `Type at position N in source is not compatible with type at position N in
//! target.`, then the inner element failure. Earlier tsz dropped the position
//! and emitted only the bare outer/inner type lines.

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

#[test]
fn tuple_element_object_property_mismatch_chains_through_position() {
    let diagnostic = ts2322(
        r#"
declare let y: [{ a: string }];
let x: [{ a: number }] = y;
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
fn nested_tuple_element_mismatch_chains_each_position() {
    let diagnostic = ts2322(
        r#"
declare let y: [[string]];
let x: [[number]] = y;
"#,
    );
    let messages = related(&diagnostic);
    // The outer tuple has one element at position 0; the inner tuple's failing
    // element is also at position 0, so the chain repeats the position line.
    let position_lines = messages
        .iter()
        .filter(|message| {
            message.as_str()
                == "Type at position 0 in source is not compatible with type at position 0 in target."
        })
        .count();
    assert!(
        position_lines >= 2,
        "expected nested position elaborations for both tuple levels; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure; related = {messages:#?}"
    );
}
