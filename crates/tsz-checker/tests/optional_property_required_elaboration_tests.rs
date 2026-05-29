//! TS2327 elaboration: "Property '_' is optional in type '_' but required in
//! type '_'." must accompany the top-level TS2322/TS2345 when a source object
//! property is present-but-optional and the target requires it.
//!
//! Structural rule: when a source property is present but optional and the
//! corresponding target property is required, tsc reports TS2327 (the property
//! is optional, not absent), not the absent-property message TS2741. The rule
//! is keyed on the optional/required modifier, so it covers inline `{ x?: T }`
//! and mapped (`Partial<T>`, `{ [K in keyof T]?: T[K] }`) sources alike, and
//! both the variable-initializer (TS2322) and call-argument (TS2345) paths.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when some top-level diagnostic with `code` carries an elaboration line
/// that is the TS2327 "is optional ... but required" message for `prop`.
fn has_optional_required_elaboration(diags: &[Diagnostic], code: u32, prop: &str) -> bool {
    let needle = format!("Property '{prop}' is optional in type ");
    diags.iter().any(|d| {
        d.code == code
            && d.related_information.iter().any(|info| {
                info.message_text.starts_with(&needle)
                    && info.message_text.contains("but required in type")
            })
    })
}

/// True when any diagnostic — as its primary message or as an elaboration
/// line — claims the property is "missing" (TS2741).
fn any_missing_message(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing in type ");
    diags.iter().any(|d| {
        d.message_text.starts_with(&needle)
            || d.related_information
                .iter()
                .any(|info| info.message_text.starts_with(&needle))
    })
}

#[test]
fn inline_optional_source_emits_ts2327() {
    let diags = diagnostics(
        r#"
declare let a: { x?: number };
declare let b: { x: number };
b = a;
"#,
    );
    assert!(
        has_optional_required_elaboration(&diags, 2322, "x"),
        "expected TS2322 with TS2327 'is optional' elaboration; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "x"),
        "must not report a present-but-optional property as 'missing'; got {diags:?}"
    );
}

#[test]
fn elaboration_is_property_name_independent() {
    // Same shape with a different property key — proves the rule is structural,
    // not keyed on the spelling 'x'.
    let diags = diagnostics(
        r#"
declare let a: { greeting?: string };
declare let b: { greeting: string };
b = a;
"#,
    );
    assert!(
        has_optional_required_elaboration(&diags, 2322, "greeting"),
        "expected TS2327 for a renamed property; got {diags:?}"
    );
}

#[test]
fn mapped_partial_source_emits_ts2327() {
    // The reported `diagnostics-35-20` shape: the optional modifier comes from a
    // mapped (`Partial`-style) output, and the optional context must survive.
    // Defined locally so the test does not depend on the lib globals.
    let diags = diagnostics(
        r#"
type Partialish<T> = { [K in keyof T]?: T[K] };
declare let a: Partialish<{ x: number }>;
declare let b: { x: number };
b = a;
"#,
    );
    assert!(
        has_optional_required_elaboration(&diags, 2322, "x"),
        "expected TS2327 for a mapped Partial-style source; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "x"),
        "must not report a mapped-optional property as 'missing'; got {diags:?}"
    );
}

#[test]
fn renamed_mapped_optional_source_emits_ts2327() {
    // Inline mapped type with a renamed iteration variable — proves the rule is
    // not tied to `Partial` by name or to any iteration-variable spelling.
    let diags = diagnostics(
        r#"
type Loosen<T> = { [Prop in keyof T]?: T[Prop] };
declare let a: Loosen<{ y: string }>;
declare let b: { y: string };
b = a;
"#,
    );
    assert!(
        has_optional_required_elaboration(&diags, 2322, "y"),
        "expected TS2327 for an inline mapped-optional source; got {diags:?}"
    );
}

#[test]
fn call_argument_optional_source_emits_ts2327() {
    // The argument path produces TS2345 at the top level but the same TS2327
    // elaboration underneath.
    let diags = diagnostics(
        r#"
type Partialish<T> = { [K in keyof T]?: T[K] };
declare function need(p: { x: number }): void;
declare let a: Partialish<{ x: number }>;
need(a);
"#,
    );
    assert!(
        has_optional_required_elaboration(&diags, 2345, "x"),
        "expected TS2345 with TS2327 elaboration; got {diags:?}"
    );
}

#[test]
fn genuinely_absent_property_still_reports_missing() {
    // Negative/guard case: an *absent* property must still report TS2741
    // ("is missing"), not TS2327 — the fix must not blur the distinction.
    let diags = diagnostics(
        r#"
declare let a: {};
declare let b: { x: number };
b = a;
"#,
    );
    assert!(
        any_missing_message(&diags, "x"),
        "an absent property must still be reported as 'missing'; got {diags:?}"
    );
    assert!(
        !has_optional_required_elaboration(&diags, 2322, "x"),
        "an absent property must not be reported as 'optional'; got {diags:?}"
    );
}

#[test]
fn matching_optionality_does_not_error() {
    // Negative case: both optional — assignable, so no diagnostic at all.
    let diags = diagnostics(
        r#"
declare let a: { x?: number };
declare let b: { x?: number };
b = a;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "optional-to-optional assignment must not error; got {diags:?}"
    );
}
