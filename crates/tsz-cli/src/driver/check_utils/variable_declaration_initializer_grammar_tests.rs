//! Unit tests for the TS1155 (`'{0}' declarations must be initialized.`)
//! classification fix — #16279 audit round 12, the #17251/#17253 regression.
//!
//! `checkGrammarVariableDeclaration` reports TS1155 from tsc's checker via
//! `grammarErrorOnNode` for an uninitialized `const` / `using` / `await using`
//! declarator on an otherwise well-formed AST. tsz emits it from the parser
//! (`state_variable_declarations.rs`), so — like every code in
//! `is_parser_grammar_code` — it must be suppressed *by* a real parse error
//! rather than counted *as* one. Before this fix TS1155 sat in
//! `is_real_syntax_error` and `is_structural_parse_error` as a stale,
//! never-matched "must be initialized" structural entry; #17251 wired the
//! parser emission and made those entries live, so an uninitialized `const`
//! both survived alongside a genuine syntax error (tsc drops it) and set
//! `has_syntax_parse_errors` / the structural-cascade flags, deleting every
//! co-occurring diagnostic in the file. That regressed 11 conformance rows
//! (`for-of2` reported only TS1155 out of `[TS1155, TS2588, TS7005]`).
//!
//! # Oracle evidence
//!
//! Pinned against `typescript@7.0.2` with
//! `--noEmit --strict --pretty false --lib es2022 --target es2022`.
//!
//! - Direction A: `const x;` alone reports TS1155 (and the semantic TS7005 —
//!   the signature of a checker grammar check that does not suppress the file's
//!   other semantic diagnostics).
//! - Direction B: `const x;` plus an unrelated real syntax error
//!   (`let y: = 1;`) drops TS1155 entirely, leaving only the structural error.
//! - Self-suppression witness: `const x;` next to
//!   `class C { get p(a: number) { return a; } }` reports BOTH TS1155 and the
//!   listed grammar sibling TS1054.

use super::*;
use tsz::parser::ParseDiagnostic;

fn diag(code: u32, message: &str) -> ParseDiagnostic {
    ParseDiagnostic {
        start: 6,
        length: 1,
        message: message.to_string(),
        code,
        related: None,
    }
}

const TS1155_MSG: &str = "'const' declarations must be initialized.";

/// TS1155 is a checker-suppressible grammar code, so it must NOT set
/// `has_syntax_parse_errors` — the flag that makes tsz drop the file's other
/// (checker-emitted) diagnostics like TS2588/TS7005. This is the damaging half
/// of the #17253 regression, pinned at the predicate the checker gate reads
/// (`check.rs` / `check_file.rs` both call `is_non_suppressing_parse_error`).
#[test]
fn ts1155_is_non_suppressing() {
    assert!(
        is_non_suppressing_parse_error(1155),
        "TS1155 is a grammar check on a well-formed AST; it must not trigger \
         has_syntax_parse_errors and delete co-occurring TS2588/TS7005"
    );
}

/// TS1155 must be classified as a parser grammar code (tsc emits it from the
/// checker), not as a real or structural syntax error. The three predicates are
/// mutually exclusive for this code after the fix.
#[test]
fn ts1155_is_a_parser_grammar_code_not_structural() {
    assert!(
        is_parser_grammar_code(1155),
        "TS1155 must live in is_parser_grammar_code"
    );
    assert!(
        !is_real_syntax_error(1155),
        "TS1155 must not remain in is_real_syntax_error (stale #17251 entry)"
    );
    assert!(
        !is_structural_parse_error(1155),
        "TS1155 must not remain in is_structural_parse_error (stale #17251 entry)"
    );
}

/// Direction B: a genuine structural parse error in the same file suppresses the
/// parser-emitted TS1155, matching tsc's `hasParseDiagnostics` short-circuit.
#[test]
fn ts1155_suppressed_by_a_structural_sibling() {
    let diagnostics = vec![
        diag(1155, TS1155_MSG),
        diag(1110, "Type expected."), // the `let y: = 1;` structural error
    ];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1155),
        "TS1155 must be suppressed by a structural TS1110 sibling, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "the structural TS1110 itself must survive, got: {codes:?}"
    );
}

/// Self-suppression witness: TS1155 co-occurring with another listed grammar
/// code (TS1054, a `get` accessor with parameters) must not delete it — before
/// the fix the misclassified TS1155 set the suppression trigger and dropped the
/// sibling. Oracle reports both codes together.
#[test]
fn ts1155_does_not_delete_a_listed_grammar_sibling() {
    let diagnostics = vec![
        diag(1054, "A 'get' accessor cannot have parameters."),
        diag(1155, TS1155_MSG),
    ];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must survive alongside TS1155, got: {codes:?}"
    );
    assert!(
        codes.contains(&1155),
        "TS1155 must survive alongside TS1054, got: {codes:?}"
    );
}

/// A lone TS1155 is always kept — the fix must never make a grammar code
/// suppress itself (the trap that blocked TS1313 before round 10).
#[test]
fn lone_ts1155_is_kept() {
    let diagnostics = vec![diag(1155, TS1155_MSG)];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![1155], "a lone TS1155 must survive");
}
