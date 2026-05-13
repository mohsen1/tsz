//! Tests for flow narrowing of logical assignment operators (&&=, ||=, ??=).
//!
//! Validates that:
//! 1. After `x ??= expr`, x is narrowed to exclude null/undefined.
//! 2. After `x ||= expr`, x is narrowed to exclude falsy types.
//! 3. After `x &&= expr`, x is NOT narrowed to truthy (may still be falsy).
//! 4. Condition narrowing works: `if (x ??= y)` narrows x in the true branch.

use crate::test_utils::check_source_strict_codes as check_strict;

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

#[test]
fn test_compound_plus_equals_preserves_number_narrowing() {
    let source = r#"
function compoundAssign(x: number | string) {
    if (typeof x === "number") {
        x += 1;
        x.toFixed();
    }
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 after x += 1 preserves number narrowing, got: {codes:?}"
    );
}

// --------------------------------------------------------------------------
// Property-access targets — issue #5937.
//
// The narrowing rule is structural: `a.b ??= c` produces a post-expression
// type of `NonNullable<typeof a.b> | typeof c` regardless of whether the LHS
// is a variable, a property access, or an element access. The same flow node
// records the assignment for all three reference kinds, so subsequent reads
// of the same reference must see the narrowed type. Earlier behavior bailed
// out before consulting `node_types[assignment_node]` whenever the LHS was a
// member-like reference, leaking TS18048 for the read that follows.
// --------------------------------------------------------------------------

#[test]
fn test_nullish_assignment_narrows_property_access_target() {
    let source = r#"
interface Config {
    options?: { timeout?: number };
}
function configure(config: Config) {
    config.options ??= {};
    config.options.timeout = 1000;
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 after `config.options ??= {{}}` narrows the property, got: {codes:?}"
    );
}

#[test]
fn test_nullish_assignment_narrows_property_access_then_compound_chain() {
    let source = r#"
interface Config {
    options?: { timeout?: number };
}
function configure(config: Config) {
    config.options ??= {};
    config.options.timeout ??= 1000;
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 after chained `??=` through property accesses, got: {codes:?}"
    );
}

#[test]
fn test_logical_or_assignment_narrows_property_access_target() {
    let source = r#"
interface Config {
    name?: string;
}
function configure(config: Config) {
    config.name ||= "default";
    return config.name.toUpperCase();
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 after `config.name ||= ...` narrows the property, got: {codes:?}"
    );
}

#[test]
fn test_nullish_assignment_narrows_element_access_target() {
    // The rule is structural: an element-access LHS with a string-literal key
    // is a member-like reference just like the matching property-access form.
    let source = r#"
interface Config {
    options?: { timeout?: number };
}
function configure(config: Config) {
    config["options"] ??= {};
    config["options"].timeout = 1000;
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 after `config[\"options\"] ??= {{}}` narrows the element access, got: {codes:?}"
    );
}

#[test]
fn test_logical_and_assignment_does_not_widen_property_access_target() {
    // `&&=` assigns only when the LHS is truthy, so `undefined` must remain.
    let source = r#"
interface Obj { a?: { b: number } }
function foo(obj: Obj) {
    obj.a &&= { b: 0 };
    obj.a.b = 42;
}
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&18048),
        "Expected TS18048 after `obj.a &&= ...` (no guaranteed assignment), got: {codes:?}"
    );
}

#[test]
fn test_nested_logical_assignment_narrows_through_property_chain() {
    // Tests the original reproduction in issue #5937 exactly. Two stacked
    // ??= operators on different depth chains must both narrow.
    let source = r#"
interface Config {
    options?: {
        timeout?: number;
        retries?: { count: number };
    };
}
function configure(config: Config) {
    config.options ??= {};
    config.options.timeout ??= 1000;
    config.options.retries ??= { count: 3 };
    config.options.retries.count = 5;
}
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&18048),
        "Expected no TS18048 after nested `??=` narrowing chain, got: {codes:?}"
    );
}
