//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

// Regression tests for contextual typing of array literals against an
// *ambiguous* union of array shapes (issue family: distributive-conditional
// tuple-union evaluation becomes over-constrained, #12175).
//
// When the contextual type is a union of two or more distinct applicable array
// shapes, the per-element contextual type used to be cleared so closures would
// fall back to implicit-any (TS7006). That also widened a nested array literal
// (`[1]` -> `number[]`), so an inner element no longer matched a tuple-element
// arm and produced a spurious TS2322. The fix threads the union of the arms'
// element types back to the element (tsc's `mapType` over a union contextual
// type), which fixes the nested-literal cases while preserving the closure
// implicit-any behavior. Each test varies a structural axis (arity, readonly,
// aliasing, mixed function/tuple arms, nesting depth) to guard against a
// single-shape fix.

fn ts2322(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics.iter().filter(|d| d.code == 2322).collect()
}

#[test]
fn union_of_tuple_arrays_nested_literal_no_false_ts2322() {
    // `[1]` must be contextually typed as the tuple `[number]` (from the first
    // arm's element type), not widened to `number[]`.
    let source = r#"
const a: [number][] | [string, boolean][] = [[1]];
const b: [number][] | [string, boolean][] = [["x", true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "union-of-tuple-arrays element should keep tuple shape, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_tuple_arrays_via_distributive_conditional() {
    // The conditional evaluates correctly to `[number][] | [string, boolean][]`;
    // the contextual element typing of `[[1]]` against it must not widen.
    let source = r#"
type ToArr<T> = T extends any ? T[] : never;
type R = ToArr<[number] | [string, boolean]>;
const a: R = [[1]];
const b: R = [["x", true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "distributive-conditional tuple-union element typing should not widen, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_readonly_tuple_arrays_nested_literal() {
    // `readonly` arms must be unwrapped for element extraction.
    let source = r#"
const a: readonly [number][] | readonly [string, boolean][] = [[1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "readonly union-of-tuple-arrays element should keep tuple shape, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_tuple_arrays_different_arity() {
    let source = r#"
const a: [number, string, boolean][] | [1][] = [[1]];
const b: [number][] | [string][] | [boolean][] = [[true]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "differing-arity union arms should still type the element per-arm, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_aliased_tuple_arrays_renamed_binders() {
    // Same shape via aliases with renamed type names: the fix must be structural,
    // not keyed on any identifier.
    let source = r#"
type Pair = [number];
type Trip = [string, boolean];
const a: Pair[] | Trip[] = [[1]];
type AltPair = [number];
type AltTrip = [string, boolean];
const b: AltPair[] | AltTrip[] = [[1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "aliased/renamed union arms should behave identically, got: {diagnostics:?}"
    );
}

#[test]
fn deeply_nested_union_of_tuple_arrays() {
    // The widening bug recurses; the contextual union must thread through each
    // nesting level.
    let source = r#"
const x: [number][][] | [string][][] = [[[1]]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "nested union-of-tuple-arrays should not widen at any depth, got: {diagnostics:?}"
    );
}

#[test]
fn union_mixed_function_and_tuple_array_picks_tuple_arm() {
    // A nested tuple literal must still pick the tuple arm even when another arm
    // is a function-typed array.
    let source = r#"
const x: ((a: number) => void)[] | [string, number][] = [["a", 1]];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "mixed fn|tuple union should let a tuple literal pick the tuple arm, got: {diagnostics:?}"
    );
}

#[test]
fn union_mixed_function_and_tuple_array_closure_picks_fn_arm() {
    // A closure element must pick the function arm (param `a: number`), so no
    // implicit-any is reported.
    let source = r#"
const x: ((a: number) => void)[] | [string, number][] = [a => {}];
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 7006 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "closure should adopt the function arm's parameter type, got: {relevant:?}"
    );
}

#[test]
fn union_of_conflicting_function_arrays_still_reports_implicit_any() {
    // When the only applicable arms are function arrays whose parameters genuinely
    // conflict, the closure parameter stays implicit-any (TS7006), matching tsc.
    let source = r#"
const x: ((a: string) => void)[] | ((a: number) => void)[] = [a => {}];
"#;
    let diagnostics = check_with_options(
        source,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 7006),
        "conflicting function-array arms should still emit TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn union_of_object_arrays_nested_literal_matches_arm() {
    let source = r#"
const a: { a: number }[] | { b: string }[] = [{ a: 1 }];
const b: { a: number }[] | { b: string }[] = [{ b: "x" }];
"#;
    let diagnostics = check_default(source);
    assert!(
        ts2322(&diagnostics).is_empty(),
        "union-of-object-arrays element should match the right arm, got: {diagnostics:?}"
    );
}
