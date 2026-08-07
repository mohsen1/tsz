//! Unit tests for the CLI parse-diagnostic post-filter's suppression *trigger*.
//!
//! `filtered_parse_diagnostics` suppresses a listed `is_parser_grammar_code`
//! diagnostic only when the file also carries a genuinely suppressing parse
//! error — tsz's stand-in for tsc's `hasParseDiagnostics(sourceFile)`. #16279
//! showed the trigger used to be the *complement* of a hand-kept six-code
//! exemption tuple plus `is_parser_grammar_code`, so any parser-emitted
//! non-suppressing code the tuple did not name (the whole `1499..=1538` regex
//! band, TS1492, TS1487, TS17019/TS17020) counted as a real parse error and
//! silently deleted every listed grammar sibling in the file.
//!
//! The trigger now routes through the single canonical
//! `is_non_suppressing_parse_error` predicate — the same one the checker gate
//! uses to set `ctx.has_syntax_parse_errors` (`check.rs`, `check_file.rs`) — so
//! the two can no longer drift.
//!
//! # Oracle evidence
//!
//! Every "keeps" witness below was pinned against `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --lib es2022 --target es2022`): each
//! non-suppressing code reports *alongside* a getter's TS1054 in the same file.
//! The discriminating control is a genuine structural error (TS1109,
//! `Expression expected.`), which drops the TS1054 in both compilers.

use super::*;
use tsz::parser::ParseDiagnostic;

/// Build a two-diagnostic file: a getter's TS1054 (a listed grammar code) plus
/// one sibling `code`. The offsets are arbitrary; only the codes drive the
/// trigger.
fn grammar_plus_sibling(code: u32, message: &str) -> Vec<ParseDiagnostic> {
    vec![
        ParseDiagnostic {
            start: 18,
            length: 2,
            message: "A 'get' accessor cannot have parameters.".to_string(),
            code: 1054,
            related: None,
        },
        ParseDiagnostic {
            start: 52,
            length: 2,
            message: message.to_string(),
            code,
            related: None,
        },
    ]
}

/// A non-suppressing sibling must never delete the getter's TS1054. Each case is
/// oracle-pinned: tsc 7.0.2 reports both codes together.
#[test]
fn non_suppressing_sibling_keeps_listed_grammar_ts1054() {
    // (sibling code, message, why it is non-suppressing)
    let cases: &[(u32, &str)] = &[
        // Regex grammar band `1499..=1538` — tsc validates the pattern from the
        // checker (scanRange), so it is never a parse diagnostic. `/abc/gg`. The
        // whole band is exhausted by `every_regex_band_code_keeps_a_listed_grammar_sibling`;
        // this is the named exemplar carrying the real tsc message.
        (1500, "A regular expression flag cannot be repeated."),
        // `using {a} = obj;` — a binding pattern on a `using` declaration; the AST
        // is valid, tsc reports it from the checker grammar phase.
        (1492, "'using' declarations may not have binding patterns."),
        // Octal escape shared with string literals — valid AST.
        (
            1487,
            "Octal escape sequences are not allowed. Use the syntax '{0}'.",
        ),
        // `?`-recovery type spellings — parser recovers a valid AST.
        (
            17019,
            "'{0}' at the end of a type is not valid TypeScript syntax.",
        ),
        (
            17020,
            "'{0}' at the start of a type is not valid TypeScript syntax.",
        ),
        // '#constructor' is a reserved word — binder/checker-raised in tsc.
        (18012, "'#constructor' is a reserved word."),
    ];

    for &(code, message) in cases {
        let diagnostics = grammar_plus_sibling(code, message);
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1054),
            "TS1054 must survive alongside non-suppressing sibling TS{code}, got: {codes:?}"
        );
        assert!(
            codes.contains(&code),
            "sibling TS{code} must survive alongside TS1054, got: {codes:?}"
        );
    }
}

/// The whole regex band, checked as a range so a newly wired regex diagnostic
/// cannot reopen the gap by omission — the same reason
/// `is_non_suppressing_parse_error` matches `1499..=1538` as a range.
#[test]
fn every_regex_band_code_keeps_a_listed_grammar_sibling() {
    for code in 1499..=1538_u32 {
        let diagnostics = grammar_plus_sibling(code, "regex grammar diagnostic");
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&1054),
            "TS1054 must survive alongside regex-band sibling TS{code}, got: {codes:?}"
        );
    }
}

/// The discriminating control: a genuine structural parse error still triggers
/// file-wide suppression of the listed grammar sibling, so the fix above is not
/// indistinguishable from deleting the trigger entirely. Oracle: `const x = ;`
/// (TS1109) drops the getter's TS1054 in tsc.
#[test]
fn structural_error_still_suppresses_listed_grammar_ts1054() {
    let diagnostics = grammar_plus_sibling(1109, "Expression expected.");
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1054),
        "TS1054 must be suppressed by a structural TS1109 sibling, got: {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "the structural TS1109 itself must survive, got: {codes:?}"
    );
}

/// A grammar code that is the file's only diagnostic is always kept — the fix
/// must not make any grammar code suppress itself.
#[test]
fn lone_grammar_code_is_kept() {
    let diagnostics = vec![ParseDiagnostic {
        start: 18,
        length: 2,
        message: "A 'get' accessor cannot have parameters.".to_string(),
        code: 1054,
        related: None,
    }];
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    assert_eq!(filtered.len(), 1, "a lone grammar code must survive");
    assert_eq!(filtered[0].code, 1054);
}

/// The CLI post-filter's trigger and the checker gate's `has_syntax_parse_errors`
/// must be the *same* function of a code, not two hand-kept lists that can drift.
/// `filtered_parse_diagnostics` suppresses a listed grammar sibling exactly when
/// some diagnostic is `!is_non_suppressing_parse_error`; assert that equivalence
/// directly over the whole diagnostic range.
#[test]
fn post_filter_trigger_matches_checker_gate_predicate() {
    for code in 0..20_000_u32 {
        let diagnostics = grammar_plus_sibling(code, "sibling");
        let filtered = filtered_parse_diagnostics(&diagnostics, false);
        let ts1054_kept = filtered.iter().any(|d| d.code == 1054);
        // The sibling suppresses TS1054 iff it is a genuinely suppressing parse
        // error — i.e. NOT non-suppressing — and is not TS1054 itself.
        let sibling_suppresses = code != 1054 && !is_non_suppressing_parse_error(code);
        assert_eq!(
            ts1054_kept, !sibling_suppresses,
            "TS1054 retention disagrees with the checker-gate predicate for sibling TS{code}"
        );
    }
}

/// The six-code exemption tuple the old complement named explicitly is fully
/// covered by `is_non_suppressing_parse_error`, so replacing the tuple is a
/// faithful superset rather than a behaviour change for those codes.
#[test]
fn legacy_exemption_tuple_is_covered_by_the_canonical_predicate() {
    for code in [1009_u32, 1185, 1214, 1262, 1359, 18012] {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} was in the old exemption tuple and must stay non-suppressing"
        );
    }
}

/// TS1260 must NOT be classified non-suppressing: it is neither structural nor a
/// grammar code, yet tsc treats it as a real parse diagnostic
/// (switchStatementsWithMultipleDefaults.ts reports only TS1260, dropping every
/// TS1113). Pins that the "default is suppressing" invariant is preserved.
#[test]
fn ts1260_stays_suppressing() {
    assert!(
        !is_non_suppressing_parse_error(1260),
        "TS1260 must remain a suppressing trigger"
    );
    let diagnostics = grammar_plus_sibling(1260, "Keyword cannot contain escape characters.");
    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    assert!(
        !filtered.iter().any(|d| d.code == 1054),
        "TS1260 must suppress a listed grammar sibling, matching tsc"
    );
}
