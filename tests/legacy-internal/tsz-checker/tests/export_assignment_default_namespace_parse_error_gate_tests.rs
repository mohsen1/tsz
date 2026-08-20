//! #16416: a namespace-body `export =` / `export default` reports TS1063 /
//! TS1319 unconditionally, even when the same statement already carries a
//! genuine parser error. tsc's `hasParseDiagnostics(sourceFile)` gate is
//! whole-file (not statement-scoped) and suppresses these grammar checks the
//! moment any real syntax error exists anywhere in the file — the sibling
//! `TS1194` branch three lines below the TS1319 site in
//! `check_export_declaration` (`state/state_checking_members/statement_callback_bridge.rs`)
//! already guards on `!self.ctx.has_parse_errors`; the TS1319 branch above it
//! did not, and `module_exports.rs`'s TS1063 site
//! (`declarations/import/core/module_exports.rs`) had no such guard at all.
//!
//! `override` is the reproducer because it is the one statement modifier
//! whose own TS1434 ("Unexpected keyword or identifier.") is never itself
//! suppressed by the parser's own container-split recovery, so it is the
//! first case where a genuine parse error and this namespace-body check
//! collide in the same statement (see #16416, split from #16412).
//!
//! All expectations measured directly against `typescript@7.0.2` (the
//! conformance pin, `scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --pretty false --lib es2022 --target es2022 --module es2022`.
//!
//! Uses [`check_source_codes_with_parse_health`], not the blind
//! `check_source`/`check_source_codes` helpers: those never wire real parser
//! diagnostics into `has_parse_errors`, so they cannot observe this
//! suppression at all (see that helper's own doc comment).

use crate::test_utils::check_source_codes_with_parse_health;

const UNEXPECTED_KEYWORD_OR_IDENTIFIER: u32 = 1434;
const EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE: u32 = 1063;
const DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE: u32 = 1319;

/// `override` before `export =` in a namespace: tsc reports TS1434 alone —
/// the parser fails on `override` before anything downstream runs, so
/// `module_exports.rs`'s namespace-body check must not also fire TS1063.
#[test]
fn override_export_assignment_in_namespace_reports_only_ts1434() {
    let codes = check_source_codes_with_parse_health("namespace N { override export = 1; }");
    assert!(
        codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER),
        "expected TS1434 for the misplaced `override`; got {codes:?}"
    );
    assert!(
        !codes.contains(&EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE),
        "tsc suppresses TS1063 once a genuine parse error exists in the file; got {codes:?}"
    );
}

/// Same shape for `export default <expr>`: tsc reports TS1434 alone, and
/// `check_export_declaration`'s TS1319 branch must defer to it.
#[test]
fn override_export_default_expression_in_namespace_reports_only_ts1434() {
    let codes = check_source_codes_with_parse_health("namespace N { override export default 1; }");
    assert!(codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(
        !codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE),
        "tsc suppresses TS1319 once a genuine parse error exists in the file; got {codes:?}"
    );
}

/// `export default class C {}` in a namespace exercises the other branch of
/// `check_export_declaration`'s TS1319 code (the `clause_is_declaration` arm
/// that anchors on the `default` keyword instead of the export node).
#[test]
fn override_export_default_class_in_namespace_reports_only_ts1434() {
    let codes =
        check_source_codes_with_parse_health("namespace N { override export default class C {} }");
    assert!(codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}

/// Same declaration-clause branch, a function this time.
#[test]
fn override_export_default_function_in_namespace_reports_only_ts1434() {
    let codes = check_source_codes_with_parse_health(
        "namespace N { override export default function f() {} }",
    );
    assert!(codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(!codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}

/// A nested namespace, and a renamed outer binder — the suppression is a
/// whole-file parse-health signal, not tied to nesting depth or a specific
/// namespace name.
#[test]
fn override_export_assignment_in_nested_namespace_reports_only_ts1434() {
    let codes = check_source_codes_with_parse_health(
        "namespace Outer { namespace Inner { override export = 1; } }",
    );
    assert!(codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(!codes.contains(&EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE));
}

/// Negative control: with no parse error present, TS1063 still fires exactly
/// as before — the gate must not over-suppress the ordinary case.
#[test]
fn export_assignment_in_namespace_without_parse_error_still_reports_ts1063() {
    let codes = check_source_codes_with_parse_health("namespace N { export = 1; }");
    assert!(!codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(codes.contains(&EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE));
}

/// Negative control for TS1319: with no parse error, it still fires.
#[test]
fn export_default_in_namespace_without_parse_error_still_reports_ts1319() {
    let codes = check_source_codes_with_parse_health("namespace N { export default 1; }");
    assert!(!codes.contains(&UNEXPECTED_KEYWORD_OR_IDENTIFIER));
    assert!(codes.contains(&DEFAULT_EXPORT_ONLY_IN_ECMASCRIPT_MODULE));
}
