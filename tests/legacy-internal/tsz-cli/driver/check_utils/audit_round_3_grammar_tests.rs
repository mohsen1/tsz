//! Unit tests for #16279 audit round 3: two unrelated single-code families
//! (TS1156, TS1358, TS18024 — see the doc comment on
//! `is_parser_grammar_code` for the oracle-derived structural rule for each,
//! including why the adjacent TS2499/TS2427/TS2457/TS2819 candidates from
//! the same scan were investigated and rejected). Split into its own file
//! rather than growing `tests.rs` past the 2000-line limit (§19).
//!
//! TS1313 was round 3's deferred candidate (self-suppression via a stale
//! mislabel in `is_real_syntax_error`/`is_structural_parse_error`); its
//! tests live below, fixed up for round 10's resolution of that trap.

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

/// #16279 audit round 10: TS1313 is now IN `is_parser_grammar_code` — it was
/// a stale mislabel (see the doc comment on `is_parser_grammar_code`) that
/// wrongly classified it as a structural parse failure, not a deliberate
/// exclusion. `program_has_real_syntax_errors` is `false` here because
/// TS1313 is no longer a member of `is_real_syntax_error`, so this does not
/// hit the self-suppression trap the old (inverted) version of this test
/// guarded against — see `ts1313_alone_does_not_flag_program_has_real_syntax_errors`
/// below for that half directly.
#[test]
fn ts1313_suppressed_when_real_parse_error_present() {
    assert_suppressed_by_real_parse_error(
        1313,
        "The body of an 'if' statement cannot be the empty statement.",
    );
}

#[test]
fn ts1313_survives_alone() {
    assert_survives_alone(
        1313,
        "The body of an 'if' statement cannot be the empty statement.",
    );
}

/// The other half of round 10's fix: TS1313 must not set
/// `program_has_real_syntax_errors`, so a lone `if (true);` does not
/// suppress unrelated cascading semantic diagnostics (tsc keeps both TS1313
/// and TS2304 for `if (true); undeclaredName;` — oracle-verified against
/// `typescript@7.0.2`). Before round 10, TS1313 was a member of
/// `is_real_syntax_error`, which made this `false`.
#[test]
fn ts1313_alone_does_not_flag_program_has_real_syntax_errors() {
    assert!(
        !is_real_syntax_error(1313),
        "TS1313 must not be a member of is_real_syntax_error — it is a \
         checker-side grammar check on a well-formed AST, not a structural \
         parse failure; tsc keeps cascading semantic diagnostics (e.g. \
         TS2304) alongside it"
    );
    assert!(
        !is_structural_parse_error(1313),
        "TS1313 must not be a member of is_structural_parse_error, same \
         reasoning as is_real_syntax_error above"
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
