//! Unit tests for the class/interface heritage-clause slice of
//! `is_parser_grammar_code` (#16279's general shape). Split into its own file
//! rather than growing `tests.rs` past the 2000-line limit (§19).
//!
//! tsc's `checkGrammarClassDeclarationHeritageClauses` /
//! `checkGrammarInterfaceDeclaration` report TS1172/1173/1174/1175/1176, all
//! parser-emitted in tsz (`parse_heritage_clause_extends` /
//! `parse_heritage_clause_implements` in
//! `state_statements_class_declarations.rs`, and the interface path in
//! `state_declarations.rs`). Before this fix, `is_parser_grammar_code` listed
//! only TS1172/1174 — TS1173 ('extends' must precede 'implements'), TS1175
//! ('implements' already seen) and TS1176 (interface cannot have
//! 'implements') were unlisted, so each counted as a "real" parse error and
//! silently deleted the listed siblings (TS1172/1174) from the same file,
//! while itself never being suppressed alongside an unrelated real syntax
//! error the way tsc suppresses it.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1173_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "'extends' clause must precede 'implements' clause.".to_string(),
            code: 1173,
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
        !codes.contains(&1173),
        "TS1173 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1175_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "'implements' clause already seen.".to_string(),
            code: 1175,
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
        !codes.contains(&1175),
        "TS1175 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1176_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "Interface declaration cannot have an 'implements' clause.".to_string(),
            code: 1176,
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
        !codes.contains(&1176),
        "TS1176 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1173_ts1175_ts1176_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for (code, message) in [
        (1173, "'extends' clause must precede 'implements' clause."),
        (1175, "'implements' clause already seen."),
        (
            1176,
            "Interface declaration cannot have an 'implements' clause.",
        ),
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 20,
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
fn filtered_parse_diagnostics_ts1173_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1173 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1172 (duplicate
    // `extends` clause).
    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 8,
            message: "'extends' clause already seen.".to_string(),
            code: 1172,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "'extends' clause must precede 'implements' clause.".to_string(),
            code: 1173,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1172),
        "TS1172 must not be self-suppressed by unlisted TS1173, got: {codes:?}"
    );
    assert!(
        codes.contains(&1173),
        "TS1173 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1175_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 8,
            message: "'extends' clause already seen.".to_string(),
            code: 1172,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "'implements' clause already seen.".to_string(),
            code: 1175,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1172),
        "TS1172 must not be self-suppressed by unlisted TS1175, got: {codes:?}"
    );
    assert!(
        codes.contains(&1175),
        "TS1175 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1176_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 8,
            message: "'extends' clause already seen.".to_string(),
            code: 1172,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "Interface declaration cannot have an 'implements' clause.".to_string(),
            code: 1176,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1172),
        "TS1172 must not be self-suppressed by unlisted TS1176, got: {codes:?}"
    );
    assert!(
        codes.contains(&1176),
        "TS1176 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}
