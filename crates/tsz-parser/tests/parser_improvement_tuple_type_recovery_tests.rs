//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — tuple type recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_named_tuple_member_rest_type_after_colon_does_not_emit_ts1005() {
    let source = r#"
type T = [first: string, rest: ...string[]?];
"#;
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Named tuple rest types after ':' should defer to later tuple diagnostics without TS1005: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_named_tuple_member_optional_type_after_colon_does_not_emit_ts1005() {
    let source = r#"
type T = [element: string?];
"#;
    let (parser, _root) = parse_source(source);

    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Named tuple members with a trailing '?' after the type should defer to later tuple diagnostics without TS1005: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn named_tuple_member_postfix_question_is_not_jsdoc_nullable() {
    let source = "type T = [a: string?];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().all(|d| {
            d.code
                != diagnostic_codes::AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE
        }),
        "Expected named tuple member `string?` to avoid TS17019, got {diagnostics:?}"
    );
}

#[test]
fn tuple_type_missing_comma_reports_comma_without_bracket_cascade() {
    let source = "type T = [string number];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let number_pos = source.find("number").expect("number token") as u32;
    assert!(
        diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::EXPECTED
                && d.start == number_pos
                && d.message == "',' expected."
        }),
        "Expected TS1005 ',' expected at `number`, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.message != "']' expected."
            && d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no bracket/TS1128 cascade, got {diagnostics:?}"
    );
}

#[test]
fn test_optional_tuple_element() {
    // [T?] should parse correctly without TS1005/TS1110
    let source = r"
interface Buzz { id: number; }
type T = [Buzz?];
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS1005 or TS1110 for optional tuple element
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1110_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1110)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for optional tuple element, got {ts1005_count}",
    );
    assert_eq!(
        ts1110_count, 0,
        "Expected no TS1110 errors for optional tuple element, got {ts1110_count}",
    );
}

#[test]
fn test_readonly_optional_tuple_element() {
    // readonly [T?] should parse correctly
    let source = r"
interface Buzz { id: number; }
type T = readonly [Buzz?];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for readonly optional tuple, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_named_tuple_element_still_works() {
    // name?: T should still parse as a named tuple element
    let source = r"
type T = [name?: string];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for named optional tuple element, got {:?}",
        parser.get_diagnostics()
    );
}

/// Run every entry of `HANGING_TUPLE_SHAPES` on a single watchdog worker
/// thread and panic if any shape fails to finish. Wakes immediately on the
/// happy path via `recv_timeout` and only blocks the full timeout on a real
/// hang. Returns nothing — diagnostic assertions run inside the worker.
fn run_hanging_tuple_matrix<F: Fn(&str) + Send + 'static>(timeout_per_shape_secs: u64, body: F) {
    use std::sync::mpsc;
    // sync_channel(1) intentionally serializes worker and watchdog one shape
    // at a time so a hang is attributed to the specific shape that hung
    // rather than to the matrix as a whole.
    let (tx, rx) = mpsc::sync_channel::<()>(1);
    let worker = std::thread::spawn(move || {
        for source in HANGING_TUPLE_SHAPES {
            body(source);
            tx.send(()).expect("watchdog channel closed");
        }
    });
    for expected in HANGING_TUPLE_SHAPES {
        if rx
            .recv_timeout(std::time::Duration::from_secs(timeout_per_shape_secs))
            .is_err()
        {
            panic!("parser hung on {expected:?} (exceeded {timeout_per_shape_secs}s)");
        }
    }
    worker.join().expect("watchdog worker panicked");
}

/// The class of inputs covered by the tuple-recovery progress guard.
///
/// Every shape here has a token inside the brackets that `can_token_start_type`
/// returns `true` for, but that neither `parse_type` nor `parse_optional(',')`
/// consume — `||`, `&&`, `==`, etc. Before the fix, the recovery branch in
/// `parse_tuple_type` would `continue` without advancing the cursor and the
/// parser would loop forever. The matrix covers multiple operators and
/// multiple tuple shapes (bare, comma-separated, named, rest, readonly,
/// nested in a type argument, and inside a mapped-type value position) so a
/// fix that only handles one spelling — or only the bare tuple form — fails.
const HANGING_TUPLE_SHAPES: &[&str] = &[
    "type T = [a||b];",
    "type T = [a&&b];",
    "type T = [a==b];",
    "type T = [a===b];",
    "type T = [a!=b];",
    "type T = [a, b||c];",
    "type T = [first: a||b];",
    "type T = [...a||b];",
    "type T = readonly [a||b];",
    "type T = Array<[a||b]>;",
    "type T<U> = { [K in keyof U]: [U[K], a||b] };",
];

#[test]
fn tuple_recovery_does_not_hang_on_binary_operator_tokens() {
    run_hanging_tuple_matrix(5, |source| {
        let (parser, _root) = crate::parser::test_fixture::parse_source(source);
        assert!(
            !parser.get_diagnostics().is_empty(),
            "expected at least one parser diagnostic for {source:?}, got none",
        );
    });
}

#[test]
fn tuple_with_double_bar_token_reports_comma_expected_without_cascade() {
    // Specific shape used by the original repro: a binary operator inside a
    // tuple element should surface as a single `',' expected.` diagnostic at
    // the operator (matching tsc), not an infinite loop or a downstream
    // bracket cascade.
    let source = "type T = [a||b];";
    let (parser, _root) = crate::parser::test_fixture::parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let op_pos = source.find("||").expect("operator present") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.message == "',' expected."
                && d.start == op_pos),
        "expected TS1005 `',' expected.` at the operator, got {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().all(|d| d.message != "']' expected."
            && d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "expected no `]`/TS1128 cascade, got {diagnostics:?}",
    );
}

#[test]
fn test_mixed_tuple_elements() {
    // Mix of optional, named, and rest elements should work
    let source = r"
interface A { a: number; }
interface B { b: string; }
type T = [A?, name: B, ...rest: string[]];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for mixed tuple elements, got {:?}",
        parser.get_diagnostics()
    );
}
