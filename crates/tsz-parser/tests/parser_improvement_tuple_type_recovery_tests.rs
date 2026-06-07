//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — tuple type recovery.

use crate::parser::test_fixture::{assert_span, parse_source};
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
fn indexed_access_private_name_tail_does_not_emit_ts1128() {
    let source = r##"
class C {
    #bar = 3;
    constructor() {
        const value: C[#bar] = 3;
    }
}
"##;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics
            .iter()
            .any(|d| { d.code == diagnostic_codes::EXPECTED && d.message == "',' expected." }),
        "expected TS1005 comma recovery for `C[#bar]`, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::VARIABLE_DECLARATION_EXPECTED),
        "expected TS1134 declaration-list recovery for `C[#bar]`, got {diagnostics:?}",
    );
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "indexed-access private-name recovery should not cascade into TS1128, got {diagnostics:?}",
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
    use std::sync::mpsc::{self, RecvTimeoutError};
    // Watchdog: the worker announces each shape over the channel *before*
    // parsing it and drops the sender after the last shape completes. The
    // main thread tracks the most recent in-flight shape and panics naming
    // it on `recv_timeout`. `thread::scope` lets the worker borrow main-
    // thread state without `'static`/`Send` bounds.
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel::<&'static str>(0);
        scope.spawn(move || {
            for source in HANGING_TUPLE_SHAPES {
                // `thread::scope` keeps the receiver alive for the entire
                // scope, so this send cannot fail.
                let _ = tx.send(source);
                let (parser, _root) = crate::parser::test_fixture::parse_source(source);
                assert!(
                    !parser.get_diagnostics().is_empty(),
                    "expected at least one parser diagnostic for {source:?}, got none",
                );
            }
        });
        let mut in_flight: &'static str = "<no shape announced>";
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(next) => in_flight = next,
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    panic!("parser hung on {in_flight:?} (exceeded 5s)")
                }
            }
        }
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

/// A named tuple member label is an *identifier name*: tsc parses it with
/// `parseIdentifierName`, so every keyword and reserved word is a legal label.
/// Before the fix, `parse_named_tuple_member` used `parse_identifier`, which
/// rejected reserved words with a spurious TS1359
/// ("'<word>' is a reserved word that cannot be used here").
///
/// The matrix below varies the label across reserved words (`in`, `function`,
/// `new`, `delete`, `for`, `if`, `typeof`, `void`, `return`), the contextual
/// keyword `readonly`, and a plain identifier so a fix that special-cases a
/// single spelling — or only the contextual-keyword case that already worked —
/// fails. Each shape covers the plain, optional, and labeled-rest forms.
const KEYWORD_TUPLE_LABELS: &[&str] = &[
    "in", "function", "new", "delete", "for", "if", "typeof", "void", "return", "readonly",
    "yield", "await", "label",
];

#[test]
fn named_tuple_member_keyword_labels_do_not_emit_ts1359() {
    for label in KEYWORD_TUPLE_LABELS {
        for source in [
            format!("type T = [{label}: string];"),
            format!("type T = [{label}?: string];"),
            format!("type T = [...{label}: string[]];"),
            format!("type T = [head: number, {label}: string];"),
        ] {
            let (parser, _root) = parse_source(&source);
            let diagnostics = parser.get_diagnostics();
            // A clean parse is the strongest guarantee — it subsumes the absence
            // of the TS1359 ("reserved word that cannot be used here") that the
            // pre-fix `parse_identifier` raised for keyword labels.
            assert!(
                diagnostics.is_empty(),
                "named tuple label {label:?} should parse cleanly (no TS1359) in {source:?}, got {diagnostics:?}",
            );
        }
    }
}

#[test]
fn named_tuple_member_keyword_label_span_covers_member_text() {
    use crate::parser::syntax_kind_ext;
    // The named-tuple-member node must span exactly `label: Type` (not overshoot
    // into the surrounding `[]`/`;`), regardless of whether the label is a
    // keyword or a plain identifier.
    for (source, member_text) in [
        ("type T = [in: string];", "in: string"),
        ("type T = [function: number];", "function: number"),
        ("type T = [...new: string[]];", "...new: string[]"),
        ("type T = [plain: string];", "plain: string"),
    ] {
        assert_span(source, syntax_kind_ext::NAMED_TUPLE_MEMBER, member_text);
    }
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
