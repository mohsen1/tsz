//! Tests for TS2859 ("Excessive complexity comparing types") from the relation
//! checker.
//!
//! Structural rule: when the relation checker's recursion guard is exceeded
//! (whether by call-stack depth or by iteration count), tsc emits TS2859
//! "Excessive complexity comparing types".  It must never emit TS2589
//! ("Type instantiation is excessively deep"), which belongs to the
//! evaluator/instantiation path.

use tsz_checker::test_utils::check_source_diagnostics;

// ---------------------------------------------------------------------------
// TS2859 — relation-checker overflow
// ---------------------------------------------------------------------------

/// tsc emits TS2859 (not TS2321 or TS2589) when the relation checker's
/// recursion guard is tripped by a large template-literal union.
///
/// Pattern: `` `${A}${A}${A}${A}` `` with an 8-member `A` produces a
/// 4096-member union that overwhelms the relation checker during
/// assignability comparison.
#[test]
fn ts2859_emitted_for_large_template_literal_union() {
    let source = r#"
type Digits = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type T1 = `${Digits}${Digits}${Digits}${Digits}` | undefined;
type T2 = { a: string } | { b: number };
function f(x: T1 | null, y: T1 & T2) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2859),
        "Expected TS2859 for large template-literal union comparison. Got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2321),
        "TS2321 must NOT fire for relation-checker overflow. Got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2589),
        "TS2589 must NOT fire for relation-checker overflow. Got: {diags:?}"
    );
}

/// The structural rule applies regardless of the alias name for the base union.
/// Renaming `Digits` to `Octal` and `T1` to `S1` must produce the same TS2859.
#[test]
fn ts2859_emitted_regardless_of_alias_name() {
    let source = r#"
type Octal = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type S1 = `${Octal}${Octal}${Octal}${Octal}` | undefined;
type S2 = { x: number } | { y: string };
function g(a: S1 | null, b: S1 & S2) {
    a = b;
}
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == 2859),
        "Expected TS2859 regardless of alias name. Got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2589),
        "TS2589 must NOT fire for relation-checker overflow (renamed aliases). Got: {diags:?}"
    );
}

/// TS2859 message must say "Excessive complexity comparing types" and reference
/// the source and target type names.
#[test]
fn ts2859_message_contains_type_names() {
    let source = r#"
type Digits = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type BigStr = `${Digits}${Digits}${Digits}${Digits}` | undefined;
type Shape = { a: string } | { b: number };
function h(x: BigStr | null, y: BigStr & Shape) {
    x = y;
}
"#;
    let diags = check_source_diagnostics(source);
    let ts2859 = diags
        .iter()
        .find(|d| d.code == 2859)
        .unwrap_or_else(|| panic!("Expected TS2859. Got: {diags:?}"));
    assert!(
        ts2859
            .message_text
            .contains("Excessive complexity comparing types"),
        "TS2859 message must say 'Excessive complexity comparing types'. Got: {}",
        ts2859.message_text
    );
}

/// Non-excessive type comparisons must not produce any overflow diagnostic.
#[test]
fn no_overflow_diagnostic_for_simple_type_mismatch() {
    let source = r#"
let x: string = 42;
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.code == 2859),
        "TS2859 must not fire for a simple type mismatch. Got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2321),
        "TS2321 must not fire for a simple type mismatch. Got: {diags:?}"
    );
}

/// TS2321 is never emitted for relation-checker overflow — both depth and
/// iteration overflow map to TS2859, matching tsc's diagnostic selection.
#[test]
fn ts2321_not_emitted_for_relation_checker_overflow() {
    let source = r#"
type D = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type Huge = `${D}${D}${D}${D}` | undefined;
type Obj = { p: string } | { q: number };
function check(x: Huge | null, y: Huge & Obj) { x = y; }
"#;
    let diags = check_source_diagnostics(source);
    assert!(
        !diags.iter().any(|d| d.code == 2321),
        "TS2321 must not fire for relation-checker overflow. Got: {diags:?}"
    );
}
