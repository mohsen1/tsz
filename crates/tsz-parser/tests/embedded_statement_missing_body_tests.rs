//! Tests for TS1109 recovery when an *embedded* statement body is missing.
//!
//! The single-statement body of `if`/`else`/`while`/`for`/`for-in`/`for-of`/
//! `with`/a labeled statement is an embedded statement position. tsc's
//! `parseStatement` always yields a node there: a token that cannot begin a
//! statement is recovered as a missing identifier that reports TS1109
//! ("Expression expected.") at that token without consuming it. tsz previously
//! forwarded the `NodeIndex::NONE` that `parse_statement` returns for such a
//! token, so the missing body went unreported (`if (x) else`, `while (x) }`)
//! or a downstream code surfaced where tsc emits none. `parse_embedded_statement`
//! now materializes the diagnostic the way tsc does.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

/// Sorted diagnostic codes for `source`.
fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// True when a TS1109 ("Expression expected.") is reported at the byte offset
/// of `marker`'s first occurrence in `source`.
fn expression_expected_at_marker(source: &str, marker: &str) -> bool {
    let pos = source
        .find(marker)
        .unwrap_or_else(|| panic!("marker {marker:?} not found in {source:?}"))
        as u32;
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .any(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED && d.start == pos)
}

fn has_ts1128(source: &str) -> bool {
    codes(source).contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
}

#[test]
fn if_then_before_else_reports_expression_expected() {
    // `if (cond)` then-clause is missing: the `else` cannot begin a statement,
    // so tsc reports TS1109 at the `else` and keeps the else-clause.
    let source = "if (cond) else foo;\n";
    assert!(
        expression_expected_at_marker(source, "else"),
        "expected TS1109 at the `else`; got {:?}",
        codes(source)
    );
    assert!(
        !has_ts1128(source),
        "missing then-clause must not report TS1128; got {:?}",
        codes(source)
    );
}

#[test]
fn if_else_missing_body_before_close_brace_reports_expression_expected() {
    // `else` body missing before the enclosing block's `}`.
    let source = "{ if (cond) foo; else }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the else-body `}}`; got {:?}",
        codes(source)
    );
    // The dedup must collapse the follow-on TS1128 at the same `}`.
    assert!(
        !has_ts1128(source),
        "else-body `}}` must not also report TS1128; got {:?}",
        codes(source)
    );
}

#[test]
fn while_missing_body_reports_expression_expected() {
    let source = "{ while (cond) }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the while-body `}}`; got {:?}",
        codes(source)
    );
    assert!(!has_ts1128(source), "got {:?}", codes(source));
}

#[test]
fn for_missing_body_reports_expression_expected() {
    let source = "{ for (;;) }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the for-body `}}`; got {:?}",
        codes(source)
    );
    assert!(!has_ts1128(source), "got {:?}", codes(source));
}

#[test]
fn for_in_missing_body_reports_expression_expected() {
    let source = "{ for (k in obj) }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the for-in-body `}}`; got {:?}",
        codes(source)
    );
    assert!(!has_ts1128(source), "got {:?}", codes(source));
}

#[test]
fn for_of_missing_body_reports_expression_expected() {
    let source = "{ for (k of arr) }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the for-of-body `}}`; got {:?}",
        codes(source)
    );
    assert!(!has_ts1128(source), "got {:?}", codes(source));
}

#[test]
fn do_missing_body_reports_expression_expected() {
    // `do } while (c)`: the do-body is missing. tsc reports TS1109 at the `}`;
    // the follow-on `'while' expected` at the same position is deduped away.
    let source = "do } while (c);\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the do-body `}}`; got {:?}",
        codes(source)
    );
}

#[test]
fn with_missing_body_reports_expression_expected() {
    // Non-strict source file so the strict-mode TS1101 does not enter the picture.
    let source = "with (obj) }\n";
    assert!(
        expression_expected_at_marker(source, "}"),
        "expected TS1109 at the with-body `}}`; got {:?}",
        codes(source)
    );
    assert!(!has_ts1128(source), "got {:?}", codes(source));
}

#[test]
fn labeled_missing_body_reports_expression_expected() {
    // Vary the label name: recovery is structural, not tied to a spelling.
    for label in ["lbl", "loop", "outer"] {
        let source = format!("{{ {label}: }}\n");
        assert!(
            expression_expected_at_marker(&source, "}"),
            "expected TS1109 at the labeled-body `}}` for {label:?}; got {:?}",
            codes(&source)
        );
        assert!(
            !has_ts1128(&source),
            "labeled-body `}}` must not also report TS1128 for {label:?}; got {:?}",
            codes(&source)
        );
    }
}

#[test]
fn valid_embedded_bodies_are_unchanged() {
    // Negative controls: well-formed bodies must not gain a spurious TS1109.
    for source in [
        "if (cond) foo; else bar;\n",
        "while (cond) foo;\n",
        "for (;;) foo;\n",
        "for (k in obj) foo;\n",
        "for (k of arr) foo;\n",
        "lbl: foo;\n",
        "do foo; while (cond);\n",
        "if (cond) { foo; } else { bar; }\n",
    ] {
        assert!(
            !codes(source).contains(&diagnostic_codes::EXPRESSION_EXPECTED),
            "well-formed body must not report TS1109 for {source:?}; got {:?}",
            codes(source)
        );
    }
}
