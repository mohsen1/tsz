//! Tests for flow narrowing of logical assignment operators (&&=, ||=, ??=).
//!
//! Validates that:
//! 1. After `x ??= expr`, x is narrowed to exclude null/undefined.
//! 2. After `x ||= expr`, x is narrowed to exclude falsy types.
//! 3. After `x &&= expr`, x is NOT narrowed to truthy (may still be falsy).
//! 4. Condition narrowing works: `if (x ??= y)` narrows x in the true branch.

use crate::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..Default::default()
    };
    crate::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// After `results ??= []`, results should be narrowed to number[] (not number[] | undefined).
/// No TS2532 (Object is possibly undefined) should be emitted on `results.push()`.
#[test]
fn test_nullish_coalescing_assignment_narrows_away_undefined() {
    let source = r#"
function foo(results: number[] | undefined) {
    results ??= [];
    results.push(100);
}
"#;
    let codes = check_strict(source);
    // Should NOT contain TS2532 (Object is possibly undefined)
    assert!(
        !codes.contains(&2532),
        "Expected no TS2532 after ??= narrowing, got codes: {codes:?}"
    );
}

/// After `results ||= []`, results should be narrowed to truthy (not undefined).
#[test]
fn test_logical_or_assignment_narrows_away_undefined() {
    let source = r#"
function foo(results: number[] | undefined) {
    results ||= [];
    results.push(100);
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2532),
        "Expected no TS2532 after ||= narrowing, got codes: {codes:?}"
    );
}

/// After `f ??= (a => a)`, f should be narrowed so f(42) doesn't trigger TS2722.
#[test]
fn test_nullish_coalescing_assignment_narrows_function() {
    let source = r#"
function foo(f?: (a: number) => void) {
    f ??= (a => a);
    f(42);
}
"#;
    let codes = check_strict(source);
    // Should NOT contain TS2722 (Cannot invoke possibly undefined)
    assert!(
        !codes.contains(&2722),
        "Expected no TS2722 after ??= narrowing on function, got codes: {codes:?}"
    );
}

/// After `f &&= (a => a)`, f is NOT guaranteed to be defined (&&= only assigns if truthy).
/// So f(42) should still trigger TS2722.
#[test]
fn test_logical_and_assignment_does_not_narrow_away_undefined() {
    let source = r#"
function foo(f?: (a: number) => void) {
    f &&= (a => a);
    f(42);
}
"#;
    let codes = check_strict(source);
    // SHOULD contain TS2722 since &&= doesn't guarantee assignment
    assert!(
        codes.contains(&2722),
        "Expected TS2722 after &&= (no guaranteed assignment), got codes: {codes:?}"
    );
}

/// Condition narrowing: `if (thing &&= expr)` should narrow thing to truthy in true branch.
#[test]
fn test_condition_and_assignment_narrows_in_true_branch() {
    let source = r#"
interface Thing { name: string; original?: Thing }
declare const v: number;
function foo(thing: Thing | undefined) {
    if (thing &&= thing) {
        thing.name;
    }
}
"#;
    let codes = check_strict(source);
    // Should NOT contain TS18048 (possibly undefined) for thing.name in true branch
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 for thing.name in if(thing &&= ...) true branch, got codes: {codes:?}"
    );
}

/// Condition narrowing: `if (thing ??= defaultValue)` should narrow thing in true branch.
#[test]
fn test_condition_nullish_assignment_narrows_in_true_branch() {
    let source = r#"
interface Thing { name: string }
function foo(thing: Thing | undefined, defaultValue: Thing | undefined) {
    if (thing ??= defaultValue) {
        thing.name;
    }
}
"#;
    let codes = check_strict(source);
    // thing.name should not trigger TS18048 — thing is narrowed to Thing in true branch
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 for thing.name in if(thing ??= ...) true branch, got codes: {codes:?}"
    );
}
