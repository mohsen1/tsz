use crate::context::CheckerOptions;
use crate::test_utils::{
    check_source_diagnostics, check_with_options, diagnostic_count, diagnostics_with_code,
    has_diagnostic_code_message,
};

#[test]
fn ts2413_static_index_signature_number_not_assignable_to_string() {
    let diags = check_source_diagnostics(
        r#"
class B {
    static readonly [s: string]: number;
    static readonly [s: number]: boolean;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2413 for static index sig mismatch, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2413_static_index_signature_compatible_no_error() {
    let diags = check_source_diagnostics(
        r#"
class C {
    static readonly [s: string]: number;
    static readonly [s: number]: 42 | 233;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
    assert_eq!(
        matching.len(),
        0,
        "Expected no TS2413 when number index is subtype of string index, got: {matching:?}"
    );
}

#[test]
fn ts2413_inherited_index_signature_conflict() {
    let diags = check_source_diagnostics(
        r#"
interface A {
    [x: string]: string;
}
interface B {
    [x: number]: number;
}
interface C extends A, B {}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
    assert!(
        !matching.is_empty(),
        "Expected TS2413 for inherited index signature conflict, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2413_base_has_both_index_sigs_no_duplicate_at_derived() {
    // interface A has conflicting own index sigs, so TS2413 is reported on A.
    // interface B extends A with no own index sigs should not get another TS2413.
    let diags = check_source_diagnostics(
        r#"
interface A {
    [n: number]: string;
    [s: string]: number;
}
interface B extends A {}
"#,
    );
    let ts2413: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
    assert_eq!(
        ts2413.len(),
        1,
        "Expected exactly 1 TS2413 (on A), not a duplicate at B's name. Got: {diags:?}"
    );
}

#[test]
fn ts2413_base_has_both_index_sigs_derived_has_own_members() {
    let diags = check_source_diagnostics(
        r#"
interface A {
    [n: number]: string;
    [s: string]: number;
}
interface B extends A {
    c: string;
    3: string;
    Infinity: string;
    "-Infinity": string;
    NaN: string;
    "-NaN": string;
    6(): string;
}
"#,
    );
    let ts2413: Vec<_> = diags.iter().filter(|d| d.code == 2413).collect();
    assert_eq!(
        ts2413.len(),
        1,
        "Expected exactly 1 TS2413 (on A), not a duplicate at B's name. Got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2411_symbol_index_signature_own_property() {
    let diags = check_source_diagnostics(
        r#"
interface I {
    [Symbol.iterator]: number;
    [s: symbol]: string;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2411).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2411 for symbol property not assignable to symbol index, got codes: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2411_symbol_index_signature_compatible_no_error() {
    let diags = check_source_diagnostics(
        r#"
interface I {
    [Symbol.iterator]: string;
    [s: symbol]: string;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2411).collect();
    assert_eq!(
        matching.len(),
        0,
        "Expected no TS2411 when symbol property is assignable to symbol index, got: {matching:?}"
    );
}

#[test]
fn ts2411_synthesized_index_from_computed_entity_names() {
    let diags = check_source_diagnostics(
        r#"
var s: string;
var n: number;
var a: any;
class C {
    [s]: number;
    [n] = n;
    [+s]: typeof s;
    [0]: number;
    [a]: number;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2411).collect();
    assert_eq!(
        matching.len(),
        2,
        "Expected 2 TS2411 for [+s] against synthesized indexes, got codes: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts2411_exact_optional_property_types_excludes_optional_declared_string() {
    let diags = check_with_options(
        r#"
interface Test {
    [key: string]: string;
    foo?: string;
    bar?: string | undefined;
}
"#,
        CheckerOptions {
            exact_optional_property_types: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        diagnostic_count(&diags, 2411),
        1,
        "Expected exactly 1 TS2411 (for bar?: string | undefined), got: {:?}",
        diagnostics_with_code(&diags, 2411)
    );
    assert!(
        has_diagnostic_code_message(&diags, 2411, "'bar'"),
        "TS2411 should be for 'bar', got: {:?}",
        diagnostics_with_code(&diags, 2411)
    );
}

#[test]
fn ts2411_exact_optional_property_types_with_renamed_optional() {
    let diags = check_with_options(
        r#"
interface MyMap {
    [key: string]: number;
    count?: number;
    value?: number | undefined;
}
"#,
        CheckerOptions {
            exact_optional_property_types: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        diagnostic_count(&diags, 2411),
        1,
        "Expected exactly 1 TS2411 (for value?: number | undefined), got: {:?}",
        diagnostics_with_code(&diags, 2411)
    );
    assert!(
        has_diagnostic_code_message(&diags, 2411, "'value'"),
        "TS2411 should be for 'value', got: {:?}",
        diagnostics_with_code(&diags, 2411)
    );
}

#[test]
fn ts2411_without_exact_optional_optional_prop_still_errors() {
    let diags = check_with_options(
        r#"
interface Test {
    [key: string]: string;
    foo?: string;
    bar?: string | undefined;
}
"#,
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        diagnostic_count(&diags, 2411),
        2,
        "Expected 2 TS2411 (both foo and bar are incompatible without EOP), got: {:?}",
        diagnostics_with_code(&diags, 2411)
    );
}
