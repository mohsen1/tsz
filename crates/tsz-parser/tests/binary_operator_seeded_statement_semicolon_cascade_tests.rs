//! A statement-list-level regression in `parse_error_for_missing_semicolon_after`
//! (`crates/tsz-parser/src/parser/state/recovery.rs`), oracle-verified against
//! `typescript@7.0.2`.
//!
//! When a statement begins with a bare binary operator that cannot start an
//! expression (`in`, `instanceof`, ...; tsc's missing-left-operand recovery,
//! `started_with_binary_operator` in `parse_expression_statement`), the
//! statement is built as `<missing> <op> <rhs>` and, if it does not find a
//! real `;`, reports its own `';' expected.` at the token it leaves
//! unconsumed. That unconsumed token becomes the START of the very next,
//! independent statement. If that next statement is itself a postfix/call
//! expression (not a plain identifier) and it *also* needs its own
//! `';' expected.` report, tsc reports both — `parseErrorAtPosition` dedups
//! only on an exact-same-start match, never on proximity.
//!
//! tsz's `should_report_error()` uses a *distance* heuristic
//! (`ERROR_SUPPRESSION_DISTANCE` = 3) instead, to suppress cascades that
//! genuinely share one root cause within a single statement's own recovery
//! (e.g. repeated stray `#!` tokens inside one malformed variable
//! initializer — see `state_statement_tests_parts/part_00.rs`'s
//! `parse_malformed_variable_hashbang_tail_matches_tsc_shape`, which still
//! must NOT regress). That heuristic wrongly also swallowed the *second*,
//! independent statement's own report whenever it landed within 3 characters
//! of the *first* statement's — exactly what happens here, since both
//! reports anchor at the same short token.
//!
//! Fixed with `binary_seeded_statement_boundary`: a narrow, one-statement-only
//! flag set *only* when `started_with_binary_operator` is true and no real
//! `;` was found, recording the boundary position. The very next statement's
//! own missing-`;` check bypasses the distance heuristic only when it lands
//! exactly on that recorded boundary — never for the `#!` family or any other
//! cascade shape, since only a bare-binary-operator-seeded statement sets the
//! flag at all.

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

const TS1005: u32 = diagnostic_codes::EXPECTED;
const TS1109: u32 = diagnostic_codes::EXPRESSION_EXPECTED;

// ---------------------------------------------------------------------------
// The reported shape: `in` at statement start recovers as
// `<missing> in get`, then the independent `x(): number;` statement follows
// immediately and needs its own missing-`;` report.
// ---------------------------------------------------------------------------

#[test]
fn binary_seeded_statement_then_call_expression_with_type_annotation_tail() {
    assert_eq!(
        fingerprints("in get x(): number;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 8), (TS1005, 1, 11)]
    );
}

/// Renamed binder: `instanceof` instead of `in`, and a differently-named
/// call target, confirms the fix is not keyed to the literal text `in`/`get`/`x`.
#[test]
fn binary_seeded_statement_renamed_binder_instanceof_then_call_expression() {
    assert_eq!(
        fingerprints("instanceof set target(): string;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 16), (TS1005, 1, 24)]
    );
}

/// Without the trailing `: number` tail, `x();` is a *complete* statement
/// (a real `;` follows the call) — tsc's statement loop finds it and reports
/// nothing further. Confirms the fix does not over-report once the second
/// statement is itself well-formed.
#[test]
fn binary_seeded_statement_then_well_formed_call_expression_reports_only_two() {
    assert_eq!(
        fingerprints("in get x();"),
        vec![(TS1109, 1, 1), (TS1005, 1, 8)]
    );
}

/// Negative control: a real `;` right after the binary-operator-seeded
/// statement's RHS means it never reaches the missing-semicolon path at all,
/// so `binary_seeded_statement_boundary` is never set and the next statement
/// gets no unwarranted treatment.
#[test]
fn binary_seeded_statement_with_real_semicolon_then_independent_statement() {
    assert_eq!(
        fingerprints("in get; x(): number;"),
        vec![(TS1109, 1, 1), (TS1005, 1, 12)]
    );
}

/// Regression guard for the adjacent, differently-shaped cascade this fix
/// must not disturb: repeated stray `#!` tokens inside one malformed `const`
/// initializer are a single statement's own recovery, not independent
/// statements, and must keep reporting all four TS18026 sites (never gated on
/// `binary_seeded_statement_boundary`, since `!` is not a binary operator).
#[test]
fn hashbang_tail_inside_one_malformed_initializer_still_reports_every_site() {
    let source =
        "const a =!@#!@$\nconst b = !@#!@#!@#!\nOK!\nHERE's A shouty thing\nGOTTA GO FAST\n";
    let diags = fingerprints(source);
    let ts18026_count = diags
        .iter()
        .filter(|(code, _, _)| *code == diagnostic_codes::CAN_ONLY_BE_USED_AT_THE_START_OF_A_FILE)
        .count();
    assert_eq!(
        ts18026_count, 4,
        "expected four TS18026 sites, got {diags:?}"
    );
}
