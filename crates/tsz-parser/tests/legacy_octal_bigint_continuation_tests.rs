//! Tests that the variable-declaration-list parser recovers `,' expected.`
//! (not `;' expected.`) when a scanner-level error inside an initializer
//! produces a trailing identifier token that can start a new declarator.
//!
//! Regression: for `{ const legacyOct = 0123n; }`, tsz used to emit
//! `;' expected.` because the generic `decl_had_error` break in
//! `parse_variable_declaration_list` preempted the "missing-comma between
//! declarators" recovery. tsc emits `,' expected.` because it scans
//! `0123` as a legacy-octal numeric literal (with TS1121) and `n` as an
//! identifier that starts a second declarator.

use crate::parser::ParserState;

fn parse_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.message.clone()))
        .collect()
}

#[test]
fn legacy_octal_bigint_in_const_emits_comma_expected_not_semicolon_expected() {
    let diags = parse_diagnostics("{ const legacyOct = 0123n; }");
    let messages: Vec<_> = diags
        .iter()
        .filter(|(c, _)| *c == 1005)
        .map(|(_, m)| m.clone())
        .collect();
    assert_eq!(
        messages,
        vec!["',' expected.".to_string()],
        "expected `,' expected.` (matching tsc's scan/recovery for legacy octal followed by `n`), got: {messages:?}"
    );
}

#[test]
fn name_level_error_still_breaks_declarator_list() {
    // `const export = 1` — `export` is reserved as a binding name → TS1389.
    // The break in parse_variable_declaration_list MUST still fire here so
    // the outer statement loop can reparse, otherwise we'd produce a stray
    // `,' expected.` and lose the original recovery shape.
    let diags = parse_diagnostics("const export = 1");
    let comma_expected = diags
        .iter()
        .filter(|(c, m)| *c == 1005 && m == "',' expected.")
        .count();
    assert_eq!(
        comma_expected, 0,
        "name-level error (reserved word `export`) must NOT trigger the comma-recovery path. \
         Got diagnostics: {diags:?}"
    );
}

#[test]
fn ok_const_with_continuation_still_continues() {
    // No error case: `const a = 1, b = 2;` — multiple declarators must
    // continue to parse after the comma.
    let diags = parse_diagnostics("const a = 1, b = 2;");
    assert!(
        diags.is_empty(),
        "valid multi-declarator declaration should produce no diagnostics, got: {diags:?}"
    );
}

#[test]
fn legacy_octal_recovery_keys_off_token_kind_not_specific_names() {
    // The fix is shape-based: any name + legacy-octal initializer + bigint-suffix
    // pattern should produce `',' expected.` regardless of identifier choice.
    for name in ["legacyOct", "x", "_a", "$bar"] {
        let source = format!("{{ const {name} = 0123n; }}");
        let diags = parse_diagnostics(&source);
        let comma_expected = diags
            .iter()
            .any(|(c, m)| *c == 1005 && m == "',' expected.");
        assert!(
            comma_expected,
            "name `{name}`: expected `,' expected.` somewhere in diagnostics, got: {diags:?}"
        );
    }
}
