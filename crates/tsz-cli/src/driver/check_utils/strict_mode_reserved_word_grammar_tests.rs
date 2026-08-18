//! Parse-stream classification of the strict-mode reserved-word family.
//!
//! tsc emits TS1212/TS1213/TS1214 from the binder (`checkStrictModeIdentifier`
//! picks the message by context in one early-return chain), routing them
//! through `getSemanticDiagnostics` — so a real parse error anywhere drops
//! them, and their own presence never suppresses anything. tsz's parser also
//! emits the family eagerly for syntactically-known strict contexts
//! (`report_strict_mode_reserved_word_error`), so the parse-stream copies must
//! be listed in `is_parser_grammar_code` to receive the same routing. TS1212
//! and TS1213 were already listed; TS1214 (module context) was not.
//!
//! Oracle evidence (typescript@6.0.2, `jsFileCompilationBindStrictModeErrors`
//! shape): a sibling file's TS1489 parse error suppresses the module file's
//! TS1214/TS1215 entirely; without the sibling both are reported alongside
//! unrelated semantic diagnostics from other files.

use super::*;

fn module_reserved_word_1214() -> ParseDiagnostic {
    ParseDiagnostic {
        start: 11,
        length: 3,
        message: "Identifier expected. 'let' is a reserved word in strict mode. \
                  Modules are automatically in strict mode."
            .to_string(),
        code: 1214,
        related: None,
    }
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1214_when_real_parse_error_present() {
    let diagnostics = vec![
        module_reserved_word_1214(),
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Expression expected.".to_string(),
            code: 1109,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1214),
        "TS1214 is semantic-phase in tsc and must be suppressed next to a real \
         parse error (TS1109), got: {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "TS1109 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1214_when_program_has_real_syntax_errors() {
    let diagnostics = vec![module_reserved_word_1214()];

    let filtered = filtered_parse_diagnostics(&diagnostics, true);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1214),
        "a real parse error in another file must also suppress TS1214, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1214_when_alone() {
    let diagnostics = vec![module_reserved_word_1214()];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1214),
        "a lone TS1214 must be reported, got: {codes:?}"
    );
}

#[test]
fn ts1214_does_not_suppress_listed_grammar_sibling() {
    let diagnostics = vec![
        module_reserved_word_1214(),
        ParseDiagnostic {
            start: 40,
            length: 2,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054) && codes.contains(&1214),
        "TS1214 is a grammar code, not a suppressing parse error — it must not \
         delete its listed TS1054 sibling, got: {codes:?}"
    );
}

/// The scanner-emitted numeric-literal family must arm the program-wide
/// syntactic gate: each of these parse errors makes `getSyntacticDiagnostics`
/// non-empty in tsc, which skips the semantic phase for every file.
#[test]
fn numeric_literal_scanner_codes_are_real_syntax_errors() {
    for code in [1125, 1177, 1178, 1352, 1353, 1489, 6188, 6189] {
        assert!(
            is_real_syntax_error(code),
            "TS{code} is a scanner-emitted parse error in tsc and must arm the \
             syntactic gate"
        );
    }
}
