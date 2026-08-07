//! Unit tests for the TS1492 slice of `is_parser_grammar_code` (#16279's
//! general shape, audit round 5).
//!
//! TS1492 (`'{0}' declarations may not have binding patterns.`,
//! `crates/tsz-parser/src/parser/state_variable_declarations.rs` —
//! `using {a} = x` / `await using [a] = x`) is the direct-declaration sibling
//! that round 4's `for...in`/`for...of` TS1493/TS1494
//! (`for_in_using_declaration_grammar_tests.rs`) left out of the same "using
//! declaration" grammar family. Before this fix, TS1492 was unlisted in
//! `is_parser_grammar_code`, so it counted as a "real" non-grammar parse
//! error under `has_non_grammar_parse_error`: never suppressed alongside a
//! genuine syntax error where tsc oracle-confirmed (`typescript@7.0.2`)
//! suppresses it, and able to silently delete an unrelated *listed* sibling
//! (e.g. TS1079, `checkGrammarModifiers`) from the same file.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1492_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 5,
            message: "'using' declarations may not have binding patterns.".to_string(),
            code: 1492,
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
        !codes.contains(&1492),
        "TS1492 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1492_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 5,
        length: 5,
        message: "'using' declarations may not have binding patterns.".to_string(),
        code: 1492,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1492),
        "TS1492 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1492_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1492 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error and silently deleted every
    // *listed* sibling in the same file — here, the already-listed TS1079
    // from an unrelated `declare`-on-an-import-declaration. Oracle-verified
    // end to end (`using {a} = null as any; export declare import x =
    // require("y");`): tsc keeps both TS1492 and TS1079, `main` dropped
    // TS1079 entirely.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 5,
            message: "'using' declarations may not have binding patterns.".to_string(),
            code: 1492,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 7,
            message: "A 'declare' modifier cannot be used with an import declaration.".to_string(),
            code: 1079,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1492),
        "TS1492 should survive when it is not the only non-grammar-looking diagnostic, got: {codes:?}"
    );
    assert!(
        codes.contains(&1079),
        "TS1079 must not be self-suppressed by unlisted TS1492, got: {codes:?}"
    );
}
