//! Unit tests for the TS1326 slice of `is_parser_grammar_code`
//! (#16279's general shape, audit round 6).
//!
//! tsc's `checkGrammarImportCallExpression` reports TS1326 ("This use of
//! 'import' is invalid. '`import()`' calls can be written, but they must have
//! parentheses and cannot have type arguments.") from the checker for
//! `import<T>("m")`. tsz emits it from the parser
//! (`crates/tsz-parser/src/parser/state_expressions_literals.rs`). Before
//! this fix, TS1326 was entirely absent from `is_parser_grammar_code`, so it
//! counted as a "real" non-grammar parse error under
//! `has_non_grammar_parse_error` and would silently delete an unrelated
//! *listed* sibling from the same file, while itself never being suppressed
//! alongside a real syntax error the way tsc suppresses it.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - Direction A: `import<number>("mod");` alone reports TS1326 (plus the
//!   unrelated TS2307 for the unresolved module specifier).
//! - Direction B: the same line plus an unrelated real syntax error
//!   (`let x: = 1;`) elsewhere in the file reports only the real syntax
//!   error (TS1110) — tsc drops TS1326 entirely, confirming it belongs in
//!   the suppression list alongside its siblings.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1326_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 40,
            length: 6,
            message: "This use of 'import' is invalid. 'import()' calls can be written, but they must have parentheses and cannot have type arguments.".to_string(),
            code: 1326,
            related: None,
        },
        ParseDiagnostic {
            start: 6,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1326),
        "TS1326 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1326_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 40,
        length: 6,
        message: "This use of 'import' is invalid. 'import()' calls can be written, but they must have parentheses and cannot have type arguments.".to_string(),
        code: 1326,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1326),
        "TS1326 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1326_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1326 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1079 (a
    // `declare`-on-an-import-declaration modifier error).
    let diagnostics = vec![
        ParseDiagnostic {
            start: 10,
            length: 7,
            message: "A 'declare' modifier cannot be used with an import declaration."
                .to_string(),
            code: 1079,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "This use of 'import' is invalid. 'import()' calls can be written, but they must have parentheses and cannot have type arguments.".to_string(),
            code: 1326,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1079),
        "TS1079 must not be self-suppressed by unlisted TS1326, got: {codes:?}"
    );
    assert!(
        codes.contains(&1326),
        "TS1326 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_ts1326() {
    assert!(is_parser_grammar_code(1326));
}
