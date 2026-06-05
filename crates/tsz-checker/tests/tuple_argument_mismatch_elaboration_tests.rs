//! Tuple-argument (`TS2345`) mismatch elaboration must match `tsc`.
//!
//! When a call argument whose type is a tuple fails to match the parameter's
//! tuple type, `tsc` attaches the same element-wise elaboration chain it uses
//! for the assignment context (`TS2322`), only beneath the `Argument of type
//! 'S' is not assignable to parameter of type 'T'.` head instead of the
//! `Type 'S' is not assignable to type 'T'.` head:
//!
//! - **multi-element tuples** disambiguate the failing slot with the `TS2626`
//!   `Type at position N in source is not compatible with type at position N in
//!   target.` line, then the inner element failure;
//! - **single-element tuples** omit the positional line and relate the element
//!   types directly;
//! - **arity gaps** attach the `Source has N element(s) but target
//!   requires/allows M.` length sub-line (`TS2618`–`TS2621`).
//!
//! Regression guard for the family where the argument path dropped the whole
//! tuple elaboration chain (objects and arrays already elaborated, tuples did
//! not), tracked in the `nextjs` / `utility-types-project` benchmark rows
//! (#11593, #10916).

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn single(source: &str, code: u32) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS{code} diagnostic, got {diagnostics:#?}"
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

fn has_position_line(diagnostic: &Diagnostic) -> bool {
    related(diagnostic)
        .iter()
        .any(|message| message.contains("in source is not compatible with type at position"))
}

/// Assert that the single TS2345 produced by `source` carries every line in
/// `expected` as a related-information elaboration line.
fn assert_arg_related(source: &str, expected: &[&str]) {
    let diagnostic = single(source, 2345);
    let messages = related(&diagnostic);
    for line in expected {
        assert!(
            has_related(&diagnostic, line),
            "missing argument-path elaboration line {line:?}; related = {messages:#?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Multi-element tuple argument: tsc drills the failing slot with the TS2626
// positional line, then the leaf element relation. The rule is structural —
// independent of the slot index and the element types — so a few distinct
// shapes (second slot, first slot, a different element-type pair) must all
// drill the same way.
// ---------------------------------------------------------------------------

#[test]
fn multi_element_tuple_arg_reports_position_and_leaf() {
    for (source, expected) in [
        (
            "declare function f(t: [string, number]): void;
             declare let y: [string, string];
             f(y);",
            [
                "Type at position 1 in source is not compatible with type at position 1 in target.",
                "Type 'string' is not assignable to type 'number'.",
            ],
        ),
        (
            "declare function f(t: [string, string]): void;
             declare let y: [boolean, string];
             f(y);",
            [
                "Type at position 0 in source is not compatible with type at position 0 in target.",
                "Type 'boolean' is not assignable to type 'string'.",
            ],
        ),
        (
            "declare function f(t: [number, boolean]): void;
             declare let y: [number, number];
             f(y);",
            [
                "Type at position 1 in source is not compatible with type at position 1 in target.",
                "Type 'number' is not assignable to type 'boolean'.",
            ],
        ),
    ] {
        assert_arg_related(source, &expected);
    }
}

// ---------------------------------------------------------------------------
// Single-element tuple argument: no positional line, element types related
// directly, then the nested property elaboration.
// ---------------------------------------------------------------------------

#[test]
fn single_element_tuple_arg_omits_position_and_relates_element_directly() {
    let diagnostic = single(
        r#"
declare function f(t: [{ a: number }]): void;
declare let y: [{ a: string }];
f(y);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        !has_position_line(&diagnostic),
        "single-element tuple argument must not emit the TS2626 positional line; related = {messages:#?}"
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

// ---------------------------------------------------------------------------
// Tuple arity gap as an argument: the `Source has N element(s) …` length line.
// ---------------------------------------------------------------------------

#[test]
fn tuple_arg_arity_gap_reports_length_sub_line() {
    // Too few elements -> "requires" (TS2618); too many -> "allows only"
    // (TS2619). Both must surface on the argument path, just as the
    // assignment path does.
    for (source, expected) in [
        (
            "declare function f(t: [string, number]): void;
             declare let y: [string];
             f(y);",
            "Source has 1 element(s) but target requires 2.",
        ),
        (
            "declare function f(t: [string, number]): void;
             declare let y: [string, number, boolean];
             f(y);",
            "Source has 3 element(s) but target allows only 2.",
        ),
    ] {
        assert_arg_related(source, &[expected]);
    }
}

// ---------------------------------------------------------------------------
// Regression guards: object/array argument elaboration is unchanged, and the
// assignment-context tuple chain is unchanged.
// ---------------------------------------------------------------------------

#[test]
fn object_argument_property_mismatch_still_elaborates() {
    let diagnostic = single(
        r#"
declare function f(t: { a: number }): void;
declare let y: { a: string };
f(y);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(&diagnostic, "Types of property 'a' are incompatible."),
        "object-argument elaboration regressed; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "object-argument leaf regressed; related = {messages:#?}"
    );
}

#[test]
fn array_argument_element_mismatch_still_elaborates() {
    let diagnostic = single(
        r#"
declare function f(t: number[]): void;
declare let y: string[];
f(y);
"#,
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "array-argument elaboration regressed; related = {:#?}",
        related(&diagnostic)
    );
}

#[test]
fn assignment_context_tuple_chain_is_unchanged() {
    // The assignment (TS2322) path is the single source of truth that the
    // argument path reuses; it must keep emitting the positional line + leaf.
    let diagnostic = single(
        r#"
declare let y: [string, string];
let x: [string, number] = y;
"#,
        2322,
    );
    assert!(
        has_related(
            &diagnostic,
            "Type at position 1 in source is not compatible with type at position 1 in target."
        ),
        "assignment-context tuple elaboration regressed; related = {:#?}",
        related(&diagnostic)
    );
}

// A structurally compatible tuple argument must not produce any diagnostic.
#[test]
fn matching_tuple_argument_has_no_diagnostic() {
    let codes: Vec<u32> = check_source_diagnostics(
        r#"
declare function f(t: [string, number]): void;
declare let y: [string, number];
f(y);
"#,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect();
    assert!(
        !codes.contains(&2345),
        "unexpected TS2345 for a matching tuple argument; got {codes:?}"
    );
}
