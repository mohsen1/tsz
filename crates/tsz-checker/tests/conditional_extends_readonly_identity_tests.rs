//! Regression tests for readonly modifier participation in
//! conditional-`extends` type identity (issue #6596).
//!
//! tsc treats conditional `extends` clauses with a stricter identity than
//! ordinary assignability. Two object types whose properties differ only in
//! the `readonly` modifier are NOT identical when used as the extends-type
//! inside the higher-order `(<T>() => T extends X ? 1 : 2)` pattern. This is
//! the mechanism that makes the `IfEquals`/`Equal` trick (and the
//! `ReadonlyKeys`/`MutableKeys` utilities built on top of it) work.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

const IF_EQUALS_PRELUDE: &str = r#"
type IfEquals<X, Y, A = X, B = never> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? A : B;
"#;

#[test]
fn if_equals_distinguishes_readonly_property_from_mutable_property() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ readonly x: number }}, {{ x: number }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat readonly vs mutable as DIFF; got: {diags:#?}"
    );
}

#[test]
fn if_equals_distinguishes_mutable_property_from_readonly_property() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ x: number }}, {{ readonly x: number }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat mutable vs readonly as DIFF; got: {diags:#?}"
    );
}

#[test]
fn if_equals_identifies_two_readonly_properties_as_equal() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ readonly x: number }}, {{ readonly x: number }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat two readonly props as EQ; got: {diags:#?}"
    );
}

#[test]
fn if_equals_identifies_two_mutable_properties_as_equal() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ x: number }}, {{ x: number }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat two mutable props as EQ; got: {diags:#?}"
    );
}

#[test]
fn if_equals_distinguishes_objects_with_one_readonly_property_difference() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<\n\
          {{ readonly a: number; b: string }},\n\
          {{ a: number; b: string }},\n\
          \"EQ\",\n\
          \"DIFF\"\n\
        >;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat one-readonly-difference as DIFF; got: {diags:#?}"
    );
}

#[test]
fn if_equals_distinguishes_readonly_under_alpha_renamed_type_parameters() {
    let source = r#"
type Equal<L, R, T = "EQ", F = "DIFF"> =
  (<U>() => U extends L ? 1 : 2) extends
  (<U>() => U extends R ? 1 : 2) ? T : F;
type R = Equal<{ readonly x: number }, { x: number }>;
const r: R = "DIFF";
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "Renamed IfEquals should still treat readonly vs mutable as DIFF; got: {diags:#?}"
    );
}

#[test]
fn if_equals_distinguishes_optional_property_from_required_property() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ x?: number }}, {{ x: number }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat optional vs required as DIFF; got: {diags:#?}"
    );
}

#[test]
fn ordinary_assignment_still_permissive_about_readonly_property() {
    let source = r#"
declare const ro: { readonly x: number };
declare const mu: { x: number };
const a: { x: number } = ro;
const b: { readonly x: number } = mu;
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "Ordinary assignment should still ignore readonly differences; got: {diags:#?}"
    );
}
