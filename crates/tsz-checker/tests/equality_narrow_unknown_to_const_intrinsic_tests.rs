//! Equality narrowing of `unknown` against a const variable typed as a
//! primitive intrinsic (e.g. `declare const aString: string`).
//!
//! tsc treats `if (u === aString)` as a guard that narrows `u: unknown` to
//! the right operand's declared primitive type. The const's annotation
//! resolves to a `TypeId::STRING` etc. intrinsic, which `is_narrowing_literal`
//! must accept as a valid comparand.

use tsz_checker::context::CheckerOptions;
use tsz_common::checker_options::JsxMode;

fn diag_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn unknown_equality_narrows_to_const_string_annotation() {
    let source = r#"
declare const u: unknown;
declare const aString: string;
if (u === aString) {
    let s: string = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 — narrowing should produce string, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_narrows_to_const_number_annotation() {
    let source = r#"
declare const u: unknown;
declare const aNumber: number;
if (u === aNumber) {
    let n: number = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for number-annotated const equality narrowing, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_narrows_to_const_boolean_annotation() {
    let source = r#"
declare const u: unknown;
declare const aBoolean: boolean;
if (u === aBoolean) {
    let b: boolean = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for boolean-annotated const equality narrowing, got: {codes:?}"
    );
}

#[test]
fn unknown_equality_param_name_independent() {
    // Locks the rule is purely structural — using a different const name
    // keeps the same narrowing behaviour.
    let source = r#"
declare const u: unknown;
declare const aDifferentName: string;
if (u === aDifferentName) {
    let s: string = u;
}
"#;
    let codes = diag_codes(source);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 regardless of const name choice, got: {codes:?}"
    );
}

#[test]
fn primitive_const_inequality_does_not_exclude_from_union_false_branch() {
    let source = r#"
declare const y: string;
function f(x: string | number) {
    if (x !== y) {
        let n: number = x;
    }
}
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 because `x !== y` with `y: string` must not narrow `x` to number, got: {codes:?}"
    );
}
