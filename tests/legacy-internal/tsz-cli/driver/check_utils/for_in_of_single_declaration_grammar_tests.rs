//! Unit tests for the `for...in`/`for...of` slice of `is_parser_grammar_code`
//! (#16279's general shape). Split into its own file rather than growing
//! `tests.rs` past the 2000-line limit (already over before this change).
//!
//! tsc's `checkGrammarForInOrForOfStatement` reports TS1091 ("Only a single
//! variable declaration is allowed in a 'for...in' statement.") and TS1188
//! (the `for...of` sibling) for a multi-declarator loop head, both
//! parser-emitted in tsz
//! (`crates/tsz-parser/src/parser/state_declarations_exports.rs`). Before
//! this fix, neither code was listed in `is_parser_grammar_code` at all, so
//! each counted as a "real" non-grammar parse error and could silently
//! delete an unrelated listed sibling from the same file, while also never
//! being suppressed itself alongside a genuine syntax error — where tsc
//! oracle-confirmed (`typescript@7.0.2`) suppresses both TS1091 and TS1188
//! when the file has an unrelated real syntax error (e.g. `let a: = 1;`).

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1091_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 16,
            length: 1,
            message: "Only a single variable declaration is allowed in a 'for...in' statement."
                .to_string(),
            code: 1091,
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
        !codes.contains(&1091),
        "TS1091 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1188_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 16,
            length: 1,
            message: "Only a single variable declaration is allowed in a 'for...of' statement."
                .to_string(),
            code: 1188,
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
        !codes.contains(&1188),
        "TS1188 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1091_ts1188_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for (code, message) in [
        (
            1091,
            "Only a single variable declaration is allowed in a 'for...in' statement.",
        ),
        (
            1188,
            "Only a single variable declaration is allowed in a 'for...of' statement.",
        ),
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 16,
            length: 1,
            message: message.to_string(),
            code,
            related: None,
        }];

        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&code),
            "TS{code} should be kept when it is the only diagnostic, got: {codes:?}"
        );
    }
}

#[test]
fn filtered_parse_diagnostics_ts1091_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1091 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1191 from an
    // unrelated import declaration.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 6,
            message: "An import declaration cannot have modifiers.".to_string(),
            code: 1191,
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
        codes.contains(&1191),
        "TS1191 must not be self-suppressed by unlisted TS1091, got: {codes:?}"
    );
    assert!(
        codes.contains(&1091),
        "TS1091 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1188_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 6,
            message: "An import declaration cannot have modifiers.".to_string(),
            code: 1191,
            related: None,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Only a single variable declaration is allowed in a 'for...of' statement."
                .to_string(),
            code: 1188,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1191),
        "TS1191 must not be self-suppressed by unlisted TS1188, got: {codes:?}"
    );
    assert!(
        codes.contains(&1188),
        "TS1188 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}
