//! Unit tests for the meta-property-name slice of `is_parser_grammar_code`
//! (#16279's general shape, meta-property audit round).
//!
//! tsc's single `checkMetaProperty` reports the invalid-meta-property-name
//! family from the checker via `grammarErrorOnNode`: TS17012 ("'{0}' is not a
//! valid meta-property for keyword '{1}'. Did you mean '{2}'?") for the
//! non-call form (`new.foo` / `import.foo`) and TS18061 (the import-defer
//! variant, "Did you mean 'meta' or 'defer'?") for the call form
//! (`import.foo()`). tsz emits both from the parser instead
//! (`crates/tsz-parser/src/parser/state_expressions_literals.rs`, which picks
//! between them on whether a `(` follows) and **not** from the checker — the
//! meta-property access path (`types/property_access_type/helpers.rs`)
//! explicitly defers it ("A separate grammar check is expected to emit
//! TS17012"), so there is no double-emission to reconcile.
//!
//! The family is all-in-or-all-out: a single file can carry both
//! (`importDefer/importMetaPropertyInvalidInCall.ts` has `import.foo();`
//! then `import.foo;`). Listing only one lets the *other* — still counted as
//! a suppressing "real parse error" — delete the listed one, which is the
//! exact regression this round's conformance run caught.
//!
//! Before this fix both were absent from `is_parser_grammar_code`, so each
//! counted as a "real" non-grammar parse error under
//! `has_non_grammar_parse_error` and would silently delete an unrelated
//! *listed* sibling from the same file, while itself never being suppressed
//! alongside a real syntax error the way tsc suppresses it.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - Direction A: `const y = import.foo;` (and `function f(){ new.foo }`)
//!   alone reports TS17012; `import.foo();` alone reports TS18061.
//! - Direction B: either construct plus an unrelated real syntax error
//!   (`let zzz: = 1;`) elsewhere in the file reports only the real syntax
//!   error (TS1110) — tsc drops the meta-property code entirely, confirming
//!   both belong in the suppression list.
//! - Self-suppression: `class C { get x(a: number){return a;} }` next to
//!   `const y = import.foo;` reports **both** TS1054 and TS17012 on tsc; tsz
//!   on `main` dropped the listed TS1054.

use super::*;

#[test]
fn filtered_parse_diagnostics_suppresses_ts17012_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 40,
            length: 3,
            message:
                "'foo' is not a valid meta-property for keyword 'import'. Did you mean 'meta'?"
                    .to_string(),
            code: 17012,
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
        !codes.contains(&17012),
        "TS17012 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_ts17012_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 40,
        length: 3,
        message: "'foo' is not a valid meta-property for keyword 'new'. Did you mean 'target'?"
            .to_string(),
        code: 17012,
        related: None,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&17012),
        "TS17012 should be kept when it is the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_ts17012_does_not_self_suppress_listed_sibling() {
    use tsz::parser::ParseDiagnostic;

    // Before the fix, TS17012 was unlisted in `is_parser_grammar_code`, so it
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
            length: 3,
            message:
                "'foo' is not a valid meta-property for keyword 'import'. Did you mean 'meta'?"
                    .to_string(),
            code: 17012,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1054),
        "TS1054 must not be self-suppressed by unlisted TS17012, got: {codes:?}"
    );
    assert!(
        codes.contains(&17012),
        "TS17012 should survive when no real parse error is present, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_ts18061_when_real_parse_error_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 40,
            length: 3,
            message:
                "'foo' is not a valid meta-property for keyword 'import'. Did you mean 'meta' or 'defer'?"
                    .to_string(),
            code: 18061,
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
        !codes.contains(&18061),
        "TS18061 should be suppressed when a real parse error (TS1110) is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1110),
        "TS1110 (real parse error) should survive, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_meta_property_family_both_survive_together() {
    use tsz::parser::ParseDiagnostic;

    // The `importDefer/importMetaPropertyInvalidInCall.ts` shape: `import.foo();`
    // (TS18061) and `import.foo;` (TS17012) in one file, no real parse error.
    // tsc keeps both; listing only one member of the family let the unlisted
    // one delete the listed one. Both must survive.
    let diagnostics = vec![
        ParseDiagnostic {
            start: 0,
            length: 3,
            message:
                "'foo' is not a valid meta-property for keyword 'import'. Did you mean 'meta' or 'defer'?"
                    .to_string(),
            code: 18061,
            related: None,
        },
        ParseDiagnostic {
            start: 20,
            length: 3,
            message:
                "'foo' is not a valid meta-property for keyword 'import'. Did you mean 'meta'?"
                    .to_string(),
            code: 17012,
            related: None,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&17012) && codes.contains(&18061),
        "both meta-property family codes must survive when no real parse error is present, got: {codes:?}"
    );
}

#[test]
fn is_parser_grammar_code_accepts_meta_property_family() {
    assert!(is_parser_grammar_code(17012));
    assert!(is_parser_grammar_code(18061));
}

#[test]
fn is_non_suppressing_parse_error_folds_in_ts17012() {
    // Containment invariant: every code `is_parser_grammar_code` accepts must
    // be non-suppressing, or it would delete its own listed siblings. TS17012
    // is now covered by construction (the predicate delegates to
    // `is_parser_grammar_code`).
    assert!(is_non_suppressing_parse_error(17012));
}

#[test]
fn ts1437_and_ts6188_stay_unlisted_genuine_parser_diagnostics() {
    // Rejected-with-evidence this round: both survive Direction B on
    // `typescript@7.0.2` (kept alongside an unrelated real syntax error), so
    // they are genuine parser diagnostics in tsc too and must NOT be treated
    // as checker-suppressible grammar codes.
    assert!(
        !is_parser_grammar_code(1437),
        "TS1437 (Namespace must be given a name) is a genuine parser diagnostic"
    );
    assert!(
        !is_parser_grammar_code(6188),
        "TS6188 (Numeric separators are not allowed here) is a genuine parser diagnostic"
    );
}
