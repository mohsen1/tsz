//! Unit tests for the TS1155 slice of `is_parser_grammar_code` (#17253).
//!
//! tsc reports TS1155 ("'{0}' declarations must be initialized.") from
//! `checkGrammarVariableDeclaration` — a check-time `grammarErrorOnNode` on a
//! well-formed `const`/`using`/`await using` declarator that lacks an
//! initializer. Since #17251 tsz emits it from the parser instead
//! (`crates/tsz-parser/src/parser/state_variable_declarations.rs`'s
//! `report_const_or_using_uninitialized`). The AST parses cleanly, so tsc:
//! - drops TS1155 when a real parse error is present elsewhere in the file
//!   (Direction B), and
//! - keeps the declarator's own semantic siblings (TS2588 "cannot assign to a
//!   constant", TS7005 implicit-any) alongside it.
//!
//! Before #17253 TS1155 was absent from `is_parser_grammar_code` and, worse,
//! mislisted in `is_real_syntax_error`/`is_structural_parse_error`. So the
//! parser-emitted copy both survived alongside a real parse error (unlike tsc)
//! and — counted as a suppressing "real parse error" — silently deleted every
//! co-occurring sibling, regressing 11 conformance rows at `0d1c93cd42`
//! (`constDeclarations-errors` lost TS2588; `for-of2` lost TS2588 and TS7005).
//! This is the same shape round 10 corrected for TS1313.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1155_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    // `decoratorOnUsing`'s shape: a recovery TS1134 ("Variable declaration
    // expected") is a real parse error, so tsc drops the co-occurring TS1155.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 6,
            length: 1,
            message: "'const' declarations must be initialized.".to_string(),
            code: 1155,
            related: None,
        },
        ParseDiagnostic {
            start: 0,
            length: 3,
            message: "Variable declaration expected.".to_string(),
            code: 1134,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1155),
        "TS1155 should be suppressed when a real parse error (TS1134) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1134),
        "TS1134 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1155_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 6,
        length: 1,
        message: "'const' declarations must be initialized.".to_string(),
        code: 1155,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1155),
        "TS1155 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1155_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before #17253, an unlisted TS1155 counted as a "real" non-grammar parse
    // error and deleted every *listed* sibling in the same file — here the
    // already-listed TS1054 (a 'get' accessor with parameters). tsc reports
    // both.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 1,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 40,
            length: 1,
            message: "'const' declarations must be initialized.".to_string(),
            code: 1155,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must not be self-suppressed by an unlisted TS1155, got: {codes:?}"
    );
    assert!(
        codes.contains(&1155),
        "TS1155 should survive when no real parse error is present, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_ts1155() {
    assert!(is_parser_grammar_code(1155));
}

#[test]
fn is_non_suppressing_parse_error_folds_in_ts1155() {
    // Containment invariant: every code `is_parser_grammar_code` accepts must be
    // non-suppressing, or it would delete its own listed siblings. TS1155 is now
    // covered by construction (the predicate delegates to
    // `is_parser_grammar_code`).
    assert!(is_non_suppressing_parse_error(1155));
}

#[test]
fn ts1155_is_not_a_real_or_structural_parse_error() {
    // The other half of #17253: `const a;` parses to a well-formed AST, so
    // TS1155 must not drive `has_real_syntax_errors` or the structural-cascade
    // heuristic (which is what deleted the TS2588/TS7005 siblings). Removed from
    // both lists, mirroring round 10's TS1313 correction.
    assert!(!is_real_syntax_error(1155));
    assert!(!is_structural_parse_error(1155));
}
