//! Unit tests for the JSX comma-operator slice of `is_parser_grammar_code`
//! (#16279's general shape).
//!
//! tsc's `checkGrammarJsxExpression` reports TS18007 ("JSX expressions may
//! not use the comma operator. Did you mean to write an array?") from the
//! checker via `grammarErrorOnNode` for a comma expression inside a JSX
//! expression container (`<div className={a, b}/>`). tsz emits it from the
//! parser instead (`crates/tsz-parser/src/parser/state_types_jsx_elements.rs`)
//! and has no checker-side counterpart for this code, so there is no
//! double-emission to reconcile.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - Direction A: `const x = <div>{a, b}</div>;` alone reports TS18007
//!   (alongside unrelated JSX/comma diagnostics unaffected by this list).
//! - Direction B: the same construct plus an unrelated real syntax error
//!   (`let zzz: = 1;`) elsewhere in the file reports only the real syntax
//!   error (TS1110) — tsc drops TS18007 entirely, confirming it belongs in
//!   the suppression list.
//!
//! Before this fix TS18007 was absent from `is_parser_grammar_code`, so it
//! counted as a "real" non-grammar parse error under
//! `has_non_grammar_parse_error` and would both survive alongside a real
//! syntax error (unlike tsc) and silently delete an unrelated *listed*
//! sibling from the same file.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts18007_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 4,
            message:
                "JSX expressions may not use the comma operator. Did you mean to write an array?"
                    .to_string(),
            code: 18007,
            related: None,
        },
        ParseDiagnostic {
            start: 6,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&18007),
        "TS18007 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts18007_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 20,
        length: 4,
        message: "JSX expressions may not use the comma operator. Did you mean to write an array?"
            .to_string(),
        code: 18007,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18007),
        "TS18007 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts18007_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS18007 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1054 (a 'get'
    // accessor with parameters). tsc reports both.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 5,
            length: 1,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 40,
            length: 4,
            message:
                "JSX expressions may not use the comma operator. Did you mean to write an array?"
                    .to_string(),
            code: 18007,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must not be self-suppressed by unlisted TS18007, got: {codes:?}"
    );
    assert!(
        codes.contains(&18007),
        "TS18007 should survive when no real parse error is present, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_ts18007() {
    assert!(is_parser_grammar_code(18007));
}

#[test]
fn is_non_suppressing_parse_error_folds_in_ts18007() {
    // Containment invariant: every code `is_parser_grammar_code` accepts must
    // be non-suppressing, or it would delete its own listed siblings. TS18007
    // is now covered by construction (the predicate delegates to
    // `is_parser_grammar_code`).
    assert!(is_non_suppressing_parse_error(18007));
}
