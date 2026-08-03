//! Unit tests for the rest-parameter slice of `is_parser_grammar_code`
//! (#16279's general shape). Split into its own file rather than growing
//! `tests.rs` past the 2000-line limit (already over before this change).
//!
//! tsc's `checkGrammarParameterList` reports TS1014 (a rest parameter must be
//! last in a parameter list), TS1047 (a rest parameter cannot be optional)
//! and TS1048 (a rest parameter cannot have an initializer) from one
//! function, all parser-emitted in tsz
//! (`crates/tsz-parser/src/parser/state_statements_class.rs`). Before this
//! fix, `is_parser_grammar_code` listed only TS1014 — TS1047/1048 were
//! unlisted, so either counted as a "real" parse error and silently deleted
//! an unrelated function's listed TS1014 from the same file, while itself
//! never being suppressed alongside an unrelated real syntax error the way
//! tsc suppresses it. TS1015 ('{0}' cannot have question mark and
//! initializer) and TS1016 (a required parameter cannot follow an optional
//! parameter) belong to the same tsc function but are checker-emitted in tsz
//! (`crates/tsz-checker/src/checkers/parameter_checker.rs`), so they must not
//! get an entry here.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts1047_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "A rest parameter cannot be optional.".to_string(),
            code: 1047,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1047),
        "TS1047 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts1048_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 6,
            message: "A rest parameter cannot have an initializer.".to_string(),
            code: 1048,
        },
        ParseDiagnostic {
            start: 60,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1048),
        "TS1048 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts1047_ts1048_when_alone() {
    use tsz::parser::ParseDiagnostic;

    for (code, message) in [
        (1047, "A rest parameter cannot be optional."),
        (1048, "A rest parameter cannot have an initializer."),
    ] {
        let diagnostics = vec![ParseDiagnostic {
            start: 20,
            length: 1,
            message: message.to_string(),
            code,
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
fn filtered_parse_diagnostics_ts1047_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS1047 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1014 from an
    // unrelated function's rest parameter.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 12,
            length: 6,
            message: "A rest parameter must be last in a parameter list.".to_string(),
            code: 1014,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "A rest parameter cannot be optional.".to_string(),
            code: 1047,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1014),
        "TS1014 must not be self-suppressed by unlisted TS1047, got: {codes:?}"
    );
    assert!(
        codes.contains(&1047),
        "TS1047 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1048_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 12,
            length: 6,
            message: "A rest parameter must be last in a parameter list.".to_string(),
            code: 1014,
        },
        ParseDiagnostic {
            start: 80,
            length: 6,
            message: "A rest parameter cannot have an initializer.".to_string(),
            code: 1048,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1014),
        "TS1014 must not be self-suppressed by unlisted TS1048, got: {codes:?}"
    );
    assert!(
        codes.contains(&1048),
        "TS1048 should survive when it is the only non-grammar-looking diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts1015_ts1016_are_not_listed() {
    // TS1015 and TS1016 belong to the same tsc `checkGrammarParameterList`
    // function as TS1014/1047/1048, but tsz emits them from the checker
    // (`parameter_checker.rs`), not the parser, so they must never reach
    // this parse-diagnostic filter's suppression list. Guard this so a
    // future edit does not fold them in by analogy with their siblings.
    assert!(!is_parser_grammar_code(1015));
    assert!(!is_parser_grammar_code(1016));
}
