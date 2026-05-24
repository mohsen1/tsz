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

/// Shared `Equal`/`Expect` harness for the `typeof`-after-logical-assignment
/// tests. `Expect<Equal<A, B>>` emits TS2344 ("does not satisfy the
/// constraint 'true'") when `A` and `B` are not the same type, so a failed
/// narrowing of the `typeof` query is observable as a TS2344 diagnostic.
const TYPEOF_EQUAL_PRELUDE: &str = r#"
type Equal<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
type Expect<T extends true> = T;
"#;

fn check_typeof_equal(body: &str) -> Vec<u32> {
    check_strict(&format!("{TYPEOF_EQUAL_PRELUDE}{body}"))
}

/// `typeof x` after `x ??= e` must reflect the narrowing (drop `undefined`),
/// matching tsc. Regression for the gap where the type-query flow path used
/// the un-narrowed declared type because the assignment expression had not
/// been cached when the query was resolved.
#[test]
fn test_typeof_after_nullish_assignment_narrows() {
    let codes = check_typeof_equal(
        r#"
declare let c: number | undefined;
c ??= 5;
type T = Expect<Equal<typeof c, number>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected typeof c == number after `c ??= 5`, got codes: {codes:?}"
    );
}

/// `typeof x` after `x ||= e` must drop `undefined` too.
#[test]
fn test_typeof_after_logical_or_assignment_narrows() {
    let codes = check_typeof_equal(
        r#"
declare let value: number | undefined;
value ||= 5;
type T = Expect<Equal<typeof value, number>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected typeof value == number after `value ||= 5`, got codes: {codes:?}"
    );
}

/// `typeof x` after `x &&= e` (`number | null`) keeps the declared shape
/// (`number | null`), matching tsc's `getAssignmentReducedType` semantics.
#[test]
fn test_typeof_after_logical_and_assignment_matches_value_position() {
    let codes = check_typeof_equal(
        r#"
declare let flag: number | null;
flag &&= 5;
type T = Expect<Equal<typeof flag, number | null>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected typeof flag == number | null after `flag &&= 5`, got codes: {codes:?}"
    );
}

/// The fix must not depend on the identifier spelling and must work for a
/// function-local `let`, a parameter, and `string`-typed values.
#[test]
fn test_typeof_after_logical_assignment_local_and_parameter_scopes() {
    let codes = check_typeof_equal(
        r#"
function local() {
  let n: number | undefined;
  n ??= 5;
  type L = Expect<Equal<typeof n, number>>;
}
function param(p: number | undefined) {
  p ||= 5;
  type P = Expect<Equal<typeof p, number>>;
}
function strings(s: string | undefined) {
  s ??= "x";
  type S = Expect<Equal<typeof s, string>>;
}
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected typeof narrowing across scopes after logical assignment, got codes: {codes:?}"
    );
}

/// Regression control: a plain `typeof` query with no preceding assignment
/// still reports the declared (un-narrowed) type, so the fix did not start
/// narrowing where it should not.
#[test]
fn test_typeof_without_assignment_keeps_declared_type() {
    let codes = check_typeof_equal(
        r#"
declare let c: number | undefined;
type T = Expect<Equal<typeof c, number | undefined>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected typeof c == number | undefined with no assignment, got codes: {codes:?}"
    );
}

/// `typeof x` after a logical assignment whose RHS is an *identifier/const*
/// (not a literal) must still reflect the whole-expression result, not the
/// bare RHS type. The `&&=` case is the discriminating one: `b &&= yb` with
/// `b: number | null` and `yb: 5` is `number | null` per tsc, whereas a
/// bare-RHS shortcut would wrongly yield `5`. Guards against the
/// ordering-sensitive cached-RHS shortcut flagged in PR #9912 review.
#[test]
fn test_typeof_after_logical_assignment_identifier_rhs_uses_whole_expression() {
    let codes = check_typeof_equal(
        r#"
declare let a: number | undefined;
declare const ya: number;
a ??= ya;
type TA = Expect<Equal<typeof a, number>>;

declare let b: number | null;
declare const yb: 5;
b &&= yb;
type TB = Expect<Equal<typeof b, number | null>>;

declare let c: string | undefined;
declare const yc: string;
c ||= yc;
type TC = Expect<Equal<typeof c, string>>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "Expected whole-expression narrowing (not bare RHS) for identifier-RHS logical assignments, got codes: {codes:?}"
    );
}
