//! Regression tests for #16416: `module_exports.rs`'s TS1063 ("An export
//! assignment cannot be used in a namespace.") and `check_export_declaration`'s
//! TS1319 ("A default export can only be used in an ECMAScript-style module.")
//! namespace-body checks must be suppressed when the same statement already
//! carries a genuine syntax error, the way the sibling TS1194
//! (`export ... from` in a namespace) and `check_grammar_module_element_context`
//! diagnostics already are.
//!
//! Background
//! ----------
//! `override` is never a valid statement modifier outside a class member, so
//! tsc's parser fails on it (TS1434) before anything downstream runs — the
//! namespace-body checks must not additionally fire for the same statement.
//! Split off #16412's pinned residual; oracle-pinned against `typescript@7.0.2`
//! with `--noEmit --strict --lib es2022 --target es2022 --module es2022`.
//!
//! Binder names are varied across cases so no fix can key on an identifier.

use tsz_checker::test_utils::check_source_codes_with_parse_health as check_source_codes;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn override_before_export_assignment_in_namespace_reports_ts1434_alone() {
    let source = "namespace Alpha { override export = 1; }\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1434),
        1,
        "expected TS1434 for the illegal `override` modifier, got {codes:?}"
    );
    assert_eq!(
        count(&codes, 1063),
        0,
        "TS1063 must not accompany a genuine parse error on the same statement, got {codes:?}"
    );
}

#[test]
fn override_before_export_default_in_namespace_reports_ts1434_alone() {
    let source = "namespace Beta { override export default 1; }\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1434),
        1,
        "expected TS1434 for the illegal `override` modifier, got {codes:?}"
    );
    assert_eq!(
        count(&codes, 1319),
        0,
        "TS1319 must not accompany a genuine parse error on the same statement, got {codes:?}"
    );
}

#[test]
fn export_assignment_in_namespace_without_parse_error_still_reports_ts1063() {
    // Control: no genuine parse error on this statement, so the namespace-body
    // check must still fire as before.
    let source = "namespace Gamma { export = 1; }\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1063),
        1,
        "expected TS1063 for a plain `export =` in a namespace, got {codes:?}"
    );
    assert_eq!(
        count(&codes, 1434),
        0,
        "no parse error expected, got {codes:?}"
    );
}

#[test]
fn export_default_in_namespace_without_parse_error_still_reports_ts1319() {
    let source = "namespace Delta { export default 1; }\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1319),
        1,
        "expected TS1319 for a plain `export default` in a namespace, got {codes:?}"
    );
    assert_eq!(
        count(&codes, 1434),
        0,
        "no parse error expected, got {codes:?}"
    );
}

#[test]
fn override_before_export_assignment_at_top_level_reports_ts1434_alone() {
    // Control: `override` is illegal at top level for an unrelated reason
    // (no enclosing class), and there is no namespace body check to suppress
    // in the first place — this must stay TS1434 alone regardless of the fix.
    let source = "override export = 1;\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1434),
        1,
        "expected TS1434 for the illegal `override` modifier, got {codes:?}"
    );
    assert_eq!(count(&codes, 1063), 0, "got {codes:?}");
}

#[test]
fn unrelated_parse_error_elsewhere_in_file_still_suppresses_ts1063() {
    // A genuine syntax error anywhere in the file sets the file-wide
    // `has_syntax_parse_errors` gate this fix relies on, matching
    // `check_grammar_module_element_context`'s existing policy.
    let source = "let x: ;\nnamespace Epsilon { export = 1; }\n";
    let codes = check_source_codes(source);
    assert_eq!(
        count(&codes, 1063),
        0,
        "a file-wide parse error must suppress TS1063 too, got {codes:?}"
    );
}
