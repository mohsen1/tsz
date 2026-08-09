//! A statement that begins with a reserved-keyword binary operator (`in`,
//! `instanceof`) with no left operand recovers as `tsc`'s
//! `<missing> <op> <rhs>` (TS1109 at the operator, the operator and its RHS
//! kept in the tree). tsc's statement-list loop then re-parses whatever
//! follows as a completely independent fresh statement — e.g. `in get
//! x(): number;` is `<missing> in get` (one statement) followed by
//! `x(): number;` (a second, unrelated statement).
//!
//! tsz's `parse_error_for_missing_semicolon_after` / `parse_semicolon`
//! suppress a "';' expected" diagnostic when a recent diagnostic was emitted
//! within `ERROR_SUPPRESSION_DISTANCE` (3 characters) of the current
//! position, to avoid cascading noise from a single failed parse. That
//! heuristic doesn't know a *new*, independently-parsed statement sits in
//! between: the first statement's own "';' expected" (reported at the second
//! statement's first token, since that's where the missing semicolon was
//! expected) sits well within 3 characters of the second statement's own
//! "';' expected" a few tokens later, so tsz dropped it — even though `tsc`'s
//! real dedup (`parseErrorAtPosition`) only ever compares against the
//! immediately preceding diagnostic's exact start position, never a
//! proximity window.
//!
//! Fixed in `crates/tsz-parser/src/parser/state_switch_recovery.rs`'s
//! `parse_expression_statement`: a missing-LHS statement now sets a one-shot
//! `force_next_missing_semicolon_error_once` flag (consumed by the very next
//! `parse_semicolon` / `parse_error_for_missing_semicolon_after` check, in
//! `crates/tsz-parser/src/parser/state/recovery.rs`), so the following
//! statement's own diagnostic is never suppressed by proximity to this one.
//!
//! The trigger condition is not just `started_with_binary_operator`: `in` is
//! recognized as a binary operator directly by `is_binary_operator()` at
//! statement start, but `instanceof` — despite being an equally reserved
//! word that can never itself start an expression — is also (incorrectly,
//! out of scope here) listed in `is_expression_start()`'s identifier-like
//! token set, so it takes the generic `parse_expression()` path instead of
//! the dedicated `started_with_binary_operator` branch. Both still reach the
//! same recovery shape and report the same TS1109 anchored at the
//! statement's own start position, so the flag is set whenever that
//! diagnostic is present, not only when `started_with_binary_operator` is
//! set.

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
