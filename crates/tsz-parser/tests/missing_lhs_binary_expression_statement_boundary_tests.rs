//! A missing-semicolon diagnostic (`';' expected`) always anchors at the first
//! token of the *next* statement, and that next statement is fully independent
//! of the one whose terminator was missing. `tsc`'s `parseErrorAtPosition`
//! dedups only against the immediately preceding diagnostic's exact start
//! position — never a distance — so the next statement's own first diagnostic
//! is always reported, even when it falls a couple of columns after the
//! missing-semicolon one.
//!
//! tsz instead suppresses any diagnostic within `ERROR_SUPPRESSION_DISTANCE`
//! (3 characters) of the last, to damp cascading noise from a single failed
//! parse. That heuristic can't tell that a *new*, independently-parsed
//! statement sits in between, so it silently dropped the next statement's own
//! error whenever it landed within three columns of the missing-semicolon
//! diagnostic. This showed up in two shapes:
//!
//!   * the missing-LHS reserved-keyword binary form — `in set y(v: number);`
//!     is `<missing> in set` (TS1109 at `in`, missing `;` at `y`) followed by
//!     `y(v: number)`, whose unexpected `:` draws `',' expected`; and
//!   * the plain missing-semicolon-between-expression-statements form —
//!     `0 y(v: number);` is `0` (missing `;` at `y`) followed by the same
//!     `y(v: number)`.
//!
//! Both share one root cause and one fix: `parse_semicolon` /
//! `parse_error_for_missing_semicolon_after` (in
//! `crates/tsz-parser/src/parser/state/recovery.rs`) now call
//! `reset_error_suppression_at_statement_boundary()` right after emitting the
//! missing-semicolon diagnostic, neutralizing the proximity window so the next
//! statement's first diagnostic is judged on its own position wherever it is
//! emitted (`parse_semicolon`, `error_comma_expected`, an argument-list
//! `parse_expected`, …). This replaces the earlier per-emit-site
//! `force_next_missing_semicolon_error_once` flag, which only reached the
//! semicolon and comma sites and only for the `in`/`instanceof` form.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

/// `(code, line, column)` fingerprints, 1-based, in report order.
fn fingerprints(source: &str) -> Vec<(u32, u32, u32)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, source);
            (diag.code, pos.line + 1, pos.character + 1)
        })
        .collect()
}

const TS1109: u32 = diagnostic_codes::EXPRESSION_EXPECTED;
const TS1005: u32 = diagnostic_codes::EXPECTED;

// ---------------------------------------------------------------------------
// `in`: the original #16291 witness. tsc: TS1109 at `in`, TS1005 at `x` (the
// first statement's missing semicolon), TS1005 at `:` (the second,
// independent `x(): number;` statement's own missing semicolon).
#[test]
fn in_missing_lhs_followed_by_call_statement_reports_both_semicolons() {
    assert_eq!(
        fingerprints("in get x(): number;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 8), (TS1005, 1, 11)],
    );
}

// `instanceof` — same shape, but reaches the flag via the diagnostic-position
// fallback rather than `started_with_binary_operator` (see module doc).
#[test]
fn instanceof_missing_lhs_followed_by_call_statement_reports_both_semicolons() {
    assert_eq!(
        fingerprints("instanceof get x(): number;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 16), (TS1005, 1, 19)],
    );
}

// Nested inside a function body (the `parse_statements()` block-list loop,
// not just the top-level `parse_source_file_statements()` loop) — and
// followed by a third, ordinary statement to confirm recovery resumes
// cleanly afterward.
#[test]
fn in_missing_lhs_inside_function_body_reports_both_semicolons() {
    assert_eq!(
        fingerprints("function f() {\n  in get x(): number;\n  const z = 1;\n}\n"),
        vec![(TS1109, 2, 3), (TS1005, 2, 10), (TS1005, 2, 13)],
    );
}

// ---------------------------------------------------------------------------
// Negative controls: shapes that must NOT gain a spurious diagnostic.

// An explicit semicolon closes the missing-LHS statement cleanly — no
// missing-semicolon diagnostic to suppress or force in the first place.
#[test]
fn in_missing_lhs_with_explicit_semicolon_reports_only_ts1109() {
    assert_eq!(fingerprints("in y;"), vec![(TS1109, 1, 1)]);
}

#[test]
fn instanceof_missing_lhs_with_explicit_semicolon_reports_only_ts1109() {
    assert_eq!(fingerprints("instanceof y;"), vec![(TS1109, 1, 1)]);
}

// The binary-expression continuation consumes through a full call chain with
// no leftover tail — again nothing for a missing-semicolon diagnostic to
// suppress.
#[test]
fn in_missing_lhs_consuming_full_member_call_chain_reports_only_ts1109() {
    assert_eq!(fingerprints("in a.b.c();"), vec![(TS1109, 1, 1)]);
}

// A genuinely malformed, unrelated statement must keep its cascading
// diagnostics suppressed: this is not a missing-LHS-binary-expression
// statement at all, so the one-shot flag must never be set for it.
#[test]
fn octal_literal_missing_semicolon_cascade_is_unaffected() {
    assert_eq!(
        fingerprints("00.5;"),
        vec![
            (
                diagnostic_codes::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX,
                1,
                1
            ),
            (TS1005, 1, 3)
        ]
    );
}

// ---------------------------------------------------------------------------
// #17062 item 1: the same cascading-suppression bug also hits `error_comma_
// expected` inside a call's argument list — not just `parse_semicolon` — when
// the following statement's own first diagnostic is a missing `,` rather than
// a missing `;`. `in set y(v: number);` is `<missing> in set` (first
// statement, TS1109 + TS1005) followed by `y(v: number)` (second, independent
// statement: `v` parses as the sole argument, then the argument list hits an
// unexpected `:` where `,` or `)` was expected).
#[test]
fn in_missing_lhs_followed_by_call_with_colon_typed_argument_reports_comma_expected() {
    assert_eq!(
        fingerprints("in set y(v: number);"),
        vec![(TS1109, 1, 1), (TS1005, 1, 8), (TS1005, 1, 11)],
    );
}

#[test]
fn instanceof_missing_lhs_followed_by_call_with_colon_typed_argument_reports_comma_expected() {
    assert_eq!(
        fingerprints("instanceof set y(v: number);"),
        vec![(TS1109, 1, 1), (TS1005, 1, 16), (TS1005, 1, 19)],
    );
}

// `get` accessor keeps working the same way (regression guard for the fix's
// sibling shape, already covered by #17052 but re-asserted here alongside
// the new `set`/comma-expected cases).
#[test]
fn in_missing_lhs_followed_by_get_call_statement_reports_both_semicolons() {
    assert_eq!(
        fingerprints("in get x(): number;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 8), (TS1005, 1, 11)],
    );
}

// ---------------------------------------------------------------------------
// #17062 item 2: `report_missing_binary_rhs` anchored a missing-RHS TS1109 at
// the EOF token's own (post-trivia) position rather than right after the
// last real token — before any trailing trivia (e.g. a final newline) is
// skipped to reach EOF. This is a general binary-expression bug, not
// specific to the `in`/`instanceof` statement-boundary recovery: it also
// reproduces for an ordinary `+`/`&&` binary expression whose RHS is missing
// at EOF.

// Bare `in` at EOF: both diagnostics anchor on line 1 (tsc: col 1, col 3 —
// right after `in`), not (2, 1).
#[test]
fn bare_in_at_eof_anchors_second_diagnostic_after_operator_not_next_line() {
    assert_eq!(fingerprints("in\n"), vec![(TS1109, 1, 1), (TS1109, 1, 3)]);
}

#[test]
fn bare_instanceof_at_eof_anchors_second_diagnostic_after_operator_not_next_line() {
    assert_eq!(
        fingerprints("instanceof\n"),
        vec![(TS1109, 1, 1), (TS1109, 1, 11)],
    );
}

// Ordinary (non-statement-boundary) binary expressions with a missing RHS at
// EOF show the identical anchor bug.
#[test]
fn plus_missing_rhs_at_eof_with_trailing_newline_anchors_after_operator() {
    assert_eq!(fingerprints("a +\n"), vec![(TS1109, 1, 4)]);
}

#[test]
fn logical_and_missing_rhs_at_eof_with_trailing_newline_anchors_after_operator() {
    assert_eq!(fingerprints("a &&\n"), vec![(TS1109, 1, 5)]);
}

// Negative controls: when a *real* token (not EOF) follows — even across a
// line break, or separated by whitespace on the same line — tsc anchors at
// that real token's own position, which already matched before this fix and
// must stay unchanged.
#[test]
fn plus_missing_rhs_at_eof_without_trailing_trivia_is_unaffected() {
    assert_eq!(fingerprints("a +"), vec![(TS1109, 1, 4)]);
}

#[test]
fn logical_and_missing_rhs_before_real_token_same_line_anchors_at_token() {
    assert_eq!(fingerprints("a && ;"), vec![(TS1109, 1, 6)]);
}

#[test]
fn logical_and_missing_rhs_before_real_token_next_line_anchors_at_token() {
    assert_eq!(fingerprints("a &&\n;"), vec![(TS1109, 2, 1)]);
}

// ---------------------------------------------------------------------------
// The plain missing-semicolon-between-expression-statements form of the same
// boundary-independence bug (no `in`/`instanceof` involved). A literal-valued
// first statement missing its `;` anchors that diagnostic at the second
// statement's first token; the second statement's own first diagnostic then
// lands a couple of columns later and used to be dropped for proximity. tsc
// reports both. The literal kind is irrelevant — what matters is that the
// missing-`;` diagnostic falls at the next statement's start — so this is
// exercised across numeric / string / bigint / template / regex first
// statements, and the second statement is a call with a malformed argument
// list (`,` expected) or a conditional (`:` expected).

#[test]
fn numeric_stmt_missing_semicolon_then_call_reports_both() {
    // `0` (missing `;` at `y`) then `y(v: number)` (`,` expected at `:`).
    assert_eq!(
        fingerprints("0 y(v: number);"),
        vec![(TS1005, 1, 3), (TS1005, 1, 6)],
    );
}

#[test]
fn string_stmt_missing_semicolon_then_call_reports_both() {
    assert_eq!(
        fingerprints("\"s\" y(v: number);"),
        vec![(TS1005, 1, 5), (TS1005, 1, 8)],
    );
}

#[test]
fn bigint_stmt_missing_semicolon_then_call_reports_both() {
    assert_eq!(
        fingerprints("0n y(v: number);"),
        vec![(TS1005, 1, 4), (TS1005, 1, 7)],
    );
}

#[test]
fn numeric_stmt_missing_semicolon_then_conditional_reports_both() {
    // Second statement's own first diagnostic is a `:` expected (conditional),
    // emitted via `parse_expected` rather than the comma/semicolon sites — the
    // uniform boundary reset covers it where the old per-site flag did not.
    assert_eq!(fingerprints("0 y?1;"), vec![(TS1005, 1, 3), (TS1005, 1, 6)],);
}

// The call name is not special: varying the second statement's callee must not
// change the shape (guards against any name-based fast path).
#[test]
fn missing_semicolon_then_call_is_callee_name_independent() {
    for name in ["y", "call", "_f"] {
        let source = format!("0 {name}(v: number);");
        let semi_col = 3;
        let comma_col = 3 + name.len() as u32 + 2; // after `<name>(v`
        assert_eq!(
            fingerprints(&source),
            vec![(TS1005, 1, semi_col), (TS1005, 1, comma_col)],
            "unexpected fingerprints for {source:?}",
        );
    }
}

// Negative control: when the second statement is itself well-formed, only the
// missing `;` is reported — the boundary reset must not manufacture a spurious
// second diagnostic.
#[test]
fn numeric_stmt_missing_semicolon_then_valid_statement_reports_only_semicolon() {
    assert_eq!(fingerprints("0 y;"), vec![(TS1005, 1, 3)]);
}

// Negative control: an explicit `;` closes the first statement cleanly, so
// there is no missing-semicolon boundary and nothing to reset — the second
// statement's diagnostic is reported by ordinary suppression rules.
#[test]
fn explicit_semicolon_between_statements_is_unaffected() {
    assert_eq!(fingerprints("0; y(v: number);"), vec![(TS1005, 1, 7)]);
}
