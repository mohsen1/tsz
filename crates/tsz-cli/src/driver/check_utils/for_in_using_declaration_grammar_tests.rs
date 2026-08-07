//! Unit tests for the TS1493/TS1494 slice of `is_parser_grammar_code`
//! (#16279's general shape, audit round 4).
//!
//! tsc's `checkGrammarForInOrForOfStatement` — the same function that reports
//! the already-listed TS1091/TS1188 — also reports TS1493 ("The left-hand
//! side of a 'for...in' statement cannot be a 'using' declaration.") and
//! TS1494 (the `await using` sibling) for `for (using x in y) {}` /
//! `for (await using x in y) {}`. Both are parser-emitted in tsz
//! (`crates/tsz-parser/src/parser/state_declarations_exports.rs`). Before
//! this fix, neither code was listed in `is_parser_grammar_code`, so each
//! counted as a "real" non-grammar parse error: never suppressed alongside a
//! genuine syntax error where tsc oracle-confirmed (`typescript@7.0.2`)
//! suppresses both, and able to silently delete an unrelated *listed*
//! sibling (e.g. TS1091 itself) from the same file.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1493_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 5,
            message:
                "The left-hand side of a 'for...in' statement cannot be a 'using' declaration."
                    .to_string(),
            code: 1493,
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
        !codes.contains(&1493),
        "TS1493 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1494_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 11,
            message:
                "The left-hand side of a 'for...in' statement cannot be an 'await using' declaration."
                    .to_string(),
            code: 1494,
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
        !codes.contains(&1494),
        "TS1494 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1493_ts1494_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for (code, message) in [
        (
            1493,
            "The left-hand side of a 'for...in' statement cannot be a 'using' declaration.",
        ),
        (
            1494,
            "The left-hand side of a 'for...in' statement cannot be an 'await using' declaration.",
        ),
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 5,
            length: 5,
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
fn filtered_parse_diagnostics_ts1493_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1493 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1091 from an
    // unrelated `for...in` loop with a multi-declarator head. Oracle-verified
    // end to end (`for (using x in y) {} for (let a, b in z) {}`): tsc keeps
    // both TS1493 and TS1091, main dropped TS1091 entirely.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 5,
            message:
                "The left-hand side of a 'for...in' statement cannot be a 'using' declaration."
                    .to_string(),
            code: 1493,
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
        codes.contains(&1493),
        "TS1493 should survive when it is not the only non-grammar-looking diagnostic, got: {codes:?}"
    );
    assert!(
        codes.contains(&1091),
        "TS1091 must not be self-suppressed by unlisted TS1493, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1494_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 11,
            message:
                "The left-hand side of a 'for...in' statement cannot be an 'await using' declaration."
                    .to_string(),
            code: 1494,
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
        codes.contains(&1494),
        "TS1494 should survive when it is not the only non-grammar-looking diagnostic, got: {codes:?}"
    );
    assert!(
        codes.contains(&1091),
        "TS1091 must not be self-suppressed by unlisted TS1494, got: {codes:?}"
    );
}
