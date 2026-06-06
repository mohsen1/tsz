//! Tuple element-mismatch elaboration must survive the call-argument (`TS2345`)
//! surface, not just the direct-assignment (`TS2322`) surface.
//!
//! Structural rule: `tsc` uses one elaboration engine for both surfaces. When a
//! tuple argument fails, the `Argument of type 'S' is not assignable to
//! parameter of type 'T'.` head is followed by the *same* nested chain a
//! `let x: T = s;` assignment would produce — the positional
//! `Type at position N in source is not compatible with type at position N in
//! target.` line, the arity (`Source has N element(s) but target requires M.`)
//! line, or, for a single-element tuple, the element relation drilled directly.
//!
//! Regression: the call-argument related-info builder had no tuple arms, so the
//! tuple failure reasons fell through to "no elaboration" — flattening the
//! index labels and hiding the failing element position under a bare `TS2345`
//! headline. These assertions pin the recovered chain and vary the binder
//! names and property keys so the rule cannot be name-hardcoded.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic(source: &str, code: u32) -> Diagnostic {
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

#[test]
fn tuple_argument_second_element_mismatch_reports_position_1() {
    let diagnostic = diagnostic(
        r#"
declare function consume(value: [string, number]): void;
declare let payload: [string, string];
consume(payload);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(
            &diagnostic,
            "Type at position 1 in source is not compatible with type at position 1 in target."
        ),
        "missing positional elaboration under TS2345; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure under TS2345; related = {messages:#?}"
    );
}

#[test]
fn tuple_argument_first_element_mismatch_reports_position_0() {
    // A different binder name and a position-0 failure prove the rule is
    // structural (keyed on the failing index), not on the spelling `payload`.
    let diagnostic = diagnostic(
        r#"
declare function accept(slot: [boolean, string]): void;
declare let tuple_value: [string, string];
accept(tuple_value);
"#,
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Type at position 0 in source is not compatible with type at position 0 in target."
        ),
        "missing position-0 elaboration under TS2345; related = {:#?}",
        related(&diagnostic)
    );
}

#[test]
fn tuple_argument_arity_mismatch_reports_element_count() {
    // A shorter source tuple fails the arity gate before any element compares;
    // the gate line must reach the TS2345 surface.
    let diagnostic = diagnostic(
        r#"
declare function take(pair: [string, number]): void;
declare let single: [string];
take(single);
"#,
        2345,
    );
    assert!(
        has_related(
            &diagnostic,
            "Source has 1 element(s) but target requires 2."
        ),
        "missing arity elaboration under TS2345; related = {:#?}",
        related(&diagnostic)
    );
}

#[test]
fn single_element_tuple_argument_drills_element_relation() {
    // A single-element tuple has no position to disambiguate, so tsc relates the
    // element types directly and recurses into the element's own failure — the
    // chain must appear under TS2345 exactly as it does under TS2322.
    let diagnostic = diagnostic(
        r#"
declare function handle(wrapper: [{ count: number }]): void;
declare let boxed: [{ count: string }];
handle(boxed);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("in source is not compatible with type at position")),
        "single-element tuple must not emit a positional line; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type '{ count: string; }' is not assignable to type '{ count: number; }'."
        ),
        "missing element-type relation header under TS2345; related = {messages:#?}"
    );
    assert!(
        has_related(&diagnostic, "Types of property 'count' are incompatible."),
        "missing nested property elaboration under TS2345; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure under TS2345; related = {messages:#?}"
    );
}

#[test]
fn tuple_nested_under_object_property_argument_keeps_position() {
    // The argument is an object whose property is a multi-element tuple. tsc
    // emits the property frame, then the tuple positional line, then the leaf —
    // previously the call-argument path stopped at the property frame's plain
    // leaf and dropped the position entirely.
    let diagnostic = diagnostic(
        r#"
declare function store(record: { entries: [string, number] }): void;
declare let bag: { entries: [string, string] };
store(bag);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(&diagnostic, "Types of property 'entries' are incompatible."),
        "missing property frame under TS2345; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type at position 1 in source is not compatible with type at position 1 in target."
        ),
        "missing nested positional elaboration under TS2345; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing leaf element failure under TS2345; related = {messages:#?}"
    );
}

#[test]
fn scalar_property_argument_keeps_two_line_chain() {
    // Anti-regression: a plain scalar property mismatch must keep its established
    // two-line `Types of property 'p' … / Type 'sp' … 'tp'.` shape — the new
    // structural-drill delegation must not perturb the high-traffic scalar path.
    let diagnostic = diagnostic(
        r#"
declare function persist(item: { weight: number }): void;
declare let cargo: { weight: string };
persist(cargo);
"#,
        2345,
    );
    let messages = related(&diagnostic);
    assert!(
        has_related(&diagnostic, "Types of property 'weight' are incompatible."),
        "missing property frame; related = {messages:#?}"
    );
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'number'."
        ),
        "missing scalar leaf; related = {messages:#?}"
    );
    assert_eq!(
        messages.len(),
        2,
        "scalar property chain must stay two lines; related = {messages:#?}"
    );
}
