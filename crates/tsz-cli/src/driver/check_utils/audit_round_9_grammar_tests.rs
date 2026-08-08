//! Unit tests for #16279 audit round 9: TS1274 (`'{0}' modifier can only
//! appear on a type parameter of a class, interface or type alias`) — see
//! the doc comment on `is_parser_grammar_code` for the oracle-derived
//! structural rule. Split into its own file rather than growing `tests.rs`
//! past the 2000-line limit (§19).

use super::*;

fn assert_suppressed_by_real_parse_error(code: u32, message: &str) {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 2,
            message: message.to_string(),
            code,
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
        !codes.contains(&code),
        "TS{code} should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

fn assert_survives_alone(code: u32, message: &str) {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 20,
        length: 2,
        message: message.to_string(),
        code,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&code),
        "TS{code} should survive when it is the only diagnostic in the file, got: {codes:?}"
    );
}

#[test]
fn ts1274_in_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        1274,
        "'in' modifier can only appear on a type parameter of a class, interface or type alias.",
    );
}

#[test]
fn ts1274_in_survives_alone() {
    assert_survives_alone(
        1274,
        "'in' modifier can only appear on a type parameter of a class, interface or type alias.",
    );
}

#[test]
fn ts1274_out_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        1274,
        "'out' modifier can only appear on a type parameter of a class, interface or type alias.",
    );
}

#[test]
fn ts1274_out_survives_alone() {
    assert_survives_alone(
        1274,
        "'out' modifier can only appear on a type parameter of a class, interface or type alias.",
    );
}
