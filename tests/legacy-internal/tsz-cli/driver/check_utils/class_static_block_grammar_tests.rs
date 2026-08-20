//! Unit tests for the class-static-block slice of `is_parser_grammar_code`
//! (#16279's general shape). Split into its own file rather than growing
//! `tests.rs` past the 2000-line limit (already over before this change).
//!
//! tsc's class-static-block grammar check reports TS18037 ('await' expression
//! cannot be used inside a class static block), TS18041 (a 'return' statement
//! cannot be used inside a class static block) and TS18054 ('await using'
//! statements cannot be used inside a class static block) for violations
//! inside the same construct, all parser-emitted in tsz
//! (`crates/tsz-parser/src/parser/state_expressions_unary.rs`,
//! `state_statements.rs`, `state_declarations_exports.rs`). Before this fix,
//! `is_parser_grammar_code` listed TS18037/TS18041 but not TS18054, so an
//! `await using` violation inside a static block counted as a "real" parse
//! error and silently deleted the listed TS18037/TS18041 diagnostics from the
//! same file, while itself never being suppressed alongside an unrelated real
//! syntax error the way tsc suppresses it.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - Direction A: a static block with both `await using x = foo();` and a
//!   bare `await bar();` reports both TS18054 and TS18037.
//! - Direction B: the same `await using` violation plus an unrelated real
//!   syntax error (`let y: = 1;`) reports only the real syntax error
//!   (TS1110) — tsc drops TS18054 entirely, confirming it belongs in the
//!   suppression list alongside its siblings.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts18054_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "'await using' statements cannot be used inside a class static block."
                .to_string(),
            code: 18054,
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
        !codes.contains(&18054),
        "TS18054 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts18054_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 20,
        length: 6,
        message: "'await using' statements cannot be used inside a class static block.".to_string(),
        code: 18054,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18054),
        "TS18054 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts18054_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS18054 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS18037 from the
    // same static block.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 12,
            length: 6,
            message: "'await' expression cannot be used inside a class static block.".to_string(),
            code: 18037,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "'await using' statements cannot be used inside a class static block."
                .to_string(),
            code: 18054,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18037),
        "TS18037 must not be self-suppressed by unlisted TS18054, got: {codes:?}"
    );
    assert!(
        codes.contains(&18054),
        "TS18054 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts18054_does_not_self_suppress_ts18041_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Same shape as above, against the other already-listed static-block
    // sibling: TS18041 (a 'return' statement inside a class static block).
    let diagnostics = vec![
        ParseDiagnostic {
            start: 12,
            length: 6,
            message: "A 'return' statement cannot be used inside a class static block.".to_string(),
            code: 18041,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "'await using' statements cannot be used inside a class static block."
                .to_string(),
            code: 18054,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18041),
        "TS18041 must not be self-suppressed by unlisted TS18054, got: {codes:?}"
    );
    assert!(
        codes.contains(&18054),
        "TS18054 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_class_static_block_family() {
    // Guard the full three-member family together so a future edit cannot
    // silently drop one while touching the others.
    assert!(is_parser_grammar_code(18037));
    assert!(is_parser_grammar_code(18041));
    assert!(is_parser_grammar_code(18054));
}
