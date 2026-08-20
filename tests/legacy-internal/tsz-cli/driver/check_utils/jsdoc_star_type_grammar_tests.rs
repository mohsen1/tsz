//! Unit tests for the `TS8020` slice of `is_parser_grammar_code`
//! (#16279's general shape, audit round 11).
//!
//! tsc's checker reports `TS8020` ("JSDoc types can only be used inside
//! documentation comments.") via `grammarErrorOnNode` for a bare `*` (JSDoc
//! "any type" syntax) used in an ordinary type position. tsz emits it
//! directly from the parser (`crates/tsz-parser/src/parser/state_types.rs`,
//! `state_types_jsx.rs`) and has no checker-side counterpart, so there is no
//! double-emission to reconcile.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - Direction A: `let x: *;` alone reports TS8020.
//! - Direction B: the same line plus an unrelated real syntax error
//!   (`let y: = 1;`) elsewhere in the file reports only the real syntax
//!   error (TS1110) — tsc drops TS8020 entirely, confirming it belongs in
//!   the suppression list.
//! - Self-suppression: `class C { get x(a: number) { return a; } }` next to
//!   `let y: *;` reports **both** TS1054 and TS8020 on tsc; before this fix
//!   tsz would have dropped the listed TS1054 because the unlisted TS8020
//!   counted as a "real" parse error.
//!
//! `TS6189` (`Multiple consecutive numeric separators are not permitted.`)
//! was also probed this round as a same-scan adjacent candidate and
//! correctly rejected: it survives Direction B on the real compiler (kept
//! alongside an unrelated syntax error), so it stays unlisted — a genuine
//! parser diagnostic in tsc, matching its already-rejected sibling TS6188.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts8020_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 7,
            length: 1,
            message: "JSDoc types can only be used inside documentation comments.".to_string(),
            code: 8020,
            related: None,
        },
        ParseDiagnostic {
            start: 20,
            length: 1,
            message: "Type expected.".to_string(),
            code: 1110,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&8020),
        "TS8020 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts8020_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 7,
        length: 1,
        message: "JSDoc types can only be used inside documentation comments.".to_string(),
        code: 8020,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&8020),
        "TS8020 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts8020_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS8020 was unlisted in `is_parser_grammar_code`, so it
    // counted as a "real" non-grammar parse error under
    // `has_non_grammar_parse_error` and silently deleted every *listed*
    // sibling in the same file — here, the already-listed TS1054 (a 'get'
    // accessor with parameters). tsc reports both.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 20,
            length: 1,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 80,
            length: 1,
            message: "JSDoc types can only be used inside documentation comments.".to_string(),
            code: 8020,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must not be self-suppressed by unlisted TS8020, got: {codes:?}"
    );
    assert!(
        codes.contains(&8020),
        "TS8020 should survive when no real parse error is present, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_ts8020() {
    assert!(is_parser_grammar_code(8020));
}

#[test]
fn is_non_suppressing_parse_error_folds_in_ts8020() {
    // Containment invariant: every code `is_parser_grammar_code` accepts must
    // be non-suppressing, or it would delete its own listed siblings. TS8020
    // is now covered by construction (the predicate delegates to
    // `is_parser_grammar_code`).
    assert!(is_non_suppressing_parse_error(8020));
}

#[test]
fn ts6189_stays_unlisted_genuine_parser_diagnostic() {
    // Rejected-with-evidence this round: survives Direction B on
    // `typescript@7.0.2` (kept alongside an unrelated real syntax error), so
    // it is a genuine parser diagnostic in tsc too and must NOT be treated as
    // checker-suppressible — matching its already-rejected sibling TS6188.
    // Tested independently rather than assumed from TS6188's membership, per
    // round 4's own caution that family membership must not be inferred from
    // a sibling.
    assert!(
        !is_parser_grammar_code(6189),
        "TS6189 (Multiple consecutive numeric separators are not permitted) is a genuine parser diagnostic"
    );
}
