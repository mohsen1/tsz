//! Unit tests for #16279 audit round 3: two unrelated single-code families
//! (TS1156, TS1358, TS18024 — see the doc comment on
//! `is_parser_grammar_code` for the oracle-derived structural rule for each,
//! including why the adjacent TS1313/TS2499/TS2427/TS2457/TS2819 candidates
//! from the same scan were investigated and rejected). Split into its own
//! file rather than growing `tests.rs` past the 2000-line limit (§19).

use super::*;

fn assert_suppressed_by_real_parse_error(code: u32, message: &str) {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
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
        length: 6,
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
fn ts1156_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        1156,
        "'let' declarations can only be declared inside a block.",
    );
}

#[test]
fn ts1156_survives_alone() {
    assert_survives_alone(
        1156,
        "'let' declarations can only be declared inside a block.",
    );
}

/// TS1313 must NOT be in `is_parser_grammar_code`, unlike its siblings above.
/// It is already a member of `is_real_syntax_error`/`is_structural_parse_error`
/// (see that doc comment), so the driver's `program_has_real_syntax_errors`
/// becomes true from TS1313 alone; adding it here would make it suppress
/// itself. This reproduces that interaction directly (passing
/// `program_has_real_syntax_errors: true`, mirroring what the driver
/// actually computes for a file whose only diagnostic is TS1313) so a future
/// attempt to add 1313 to `is_parser_grammar_code` fails this test instead
/// of silently regressing `if (true) ;` to report nothing.
#[test]
fn ts1313_is_not_suppressed_and_must_stay_out_of_the_list() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 20,
        length: 6,
        message: "The body of an 'if' statement cannot be the empty statement.".to_string(),
        code: 1313,
        related: None,
    }];

    // `true` mirrors `program_has_real_syntax_errors(program)` being true
    // because TS1313 itself is (pre-existing) classified by
    // `is_real_syntax_error`, independent of `is_parser_grammar_code`.
    let filtered = filtered_parse_diagnostics(&diagnostics, true);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1313),
        "TS1313 must survive here — if this fails, TS1313 was added to \
         is_parser_grammar_code and now self-suppresses via program_has_real_syntax_errors, \
         got: {codes:?}"
    );
}

#[test]
fn ts1358_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        1358,
        "Tagged template expressions are not permitted in an optional chain.",
    );
}

#[test]
fn ts1358_survives_alone() {
    assert_survives_alone(
        1358,
        "Tagged template expressions are not permitted in an optional chain.",
    );
}

#[test]
fn ts18024_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        18024,
        "An enum member cannot be named with a private identifier.",
    );
}

#[test]
fn ts18024_survives_alone() {
    assert_survives_alone(
        18024,
        "An enum member cannot be named with a private identifier.",
    );
}

/// Negative control: TS2819 (namespace reserved names) was oracle-tested
/// and rejected — tsc keeps it alongside an unrelated real syntax error, so
/// it must NOT be in `is_parser_grammar_code` and must not be suppressed.
#[test]
fn ts2819_not_suppressed_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "Namespace name cannot be 'true'.".to_string(),
            code: 2819,
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
        codes.contains(&2819),
        "TS2819 must survive a real parse error, matching tsc's oracle behavior; got: {codes:?}"
    );
}
