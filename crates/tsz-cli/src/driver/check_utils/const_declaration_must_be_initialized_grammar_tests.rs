//! Unit tests for #17253: TS1155 (`'{0}' declarations must be initialized.`)
//! was wired into the parser by #17251 without being added to
//! `is_parser_grammar_code`. tsc's `checkGrammarVariableDeclaration` reports
//! this from the checker for an uninitialized `const` binding; tsz emits it
//! from the parser
//! (`crates/tsz-parser/src/parser/state_variable_declarations.rs`). Unlisted,
//! it both survived alongside a genuine syntax error in the same file (tsc
//! drops it) and counted as a "real" non-grammar parse error, silently
//! deleting listed grammar siblings and — via `has_syntax_parse_errors` —
//! unrelated checker diagnostics (TS2588/TS7005) elsewhere in the file.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1155_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 6,
            length: 1,
            message: "'const' declarations must be initialized.".to_string(),
            code: 1155,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1155),
        "TS1155 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
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

    // Before the fix, TS1155 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1091 from an
    // unrelated `for...in` multi-declarator loop head.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 6,
            message: "'const' declarations must be initialized.".to_string(),
            code: 1155,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Only a single variable declaration is allowed in a 'for...in' statement."
                .to_string(),
            code: 1091,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1091),
        "TS1091 must not be self-suppressed by unlisted TS1155, got: {codes:?}"
    );
    assert!(
        codes.contains(&1155),
        "TS1155 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}
