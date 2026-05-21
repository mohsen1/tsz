//! Structural regression coverage for `thislessFunctionsNotContextSensitive*`
//! (issue #8711).
//!
//! Pins the rule: a function expression or arrow that does not reference
//! `this` (and is not otherwise context-sensitive) must NOT silently fall
//! back to `any` for an outer call's generic parameters. Round-1 / Round-2
//! inference threads the contextual parameter type through the thisless
//! function exactly as if it were an arrow.
//!
//! Tests use multiple identifier spellings for type parameters and aliases
//! per CLAUDE.md §25 — if a rename breaks the rule, the fix is hardcoded.

use tsz_checker::test_utils::{
    check_source_diagnostics, diagnostic_code_message_refs, has_any_diagnostic_code,
};

const INFERENCE_FAILURE_CODES: &[u32] = &[
    2322,  // Type 'X' is not assignable to type 'Y'
    2345,  // Argument of type 'X' is not assignable to parameter of type 'Y'
    2339,  // Property 'foo' does not exist on type 'X'
    2769,  // No overload matches this call
    7006,  // Parameter 'x' implicitly has an 'any' type
    7044,  // Parameter 'x' implicitly has an 'any[]' type
    18046, // 'x' is of type 'unknown'
];

fn assert_no_inference_failure(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !has_any_diagnostic_code(&diagnostics, INFERENCE_FAILURE_CODES),
        "{context}: expected no inference-failure diagnostics, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

/// Issue #8711 repro: T must infer to `number` from the array literal.
/// A silent fallback to T = any would not flag this code either, so the
/// assertion guards against the inference-failure family rather than a
/// specific tsc diagnostic.
#[test]
fn arrow_callback_infers_element_type_from_contextual_array_param() {
    assert_no_inference_failure(
        r#"
declare function map<T, U>(xs: T[], f: (x: T) => U): U[];
const r: string[] = map([1, 2, 3], x => x.toFixed());
"#,
        "arrow + unannotated x, T must infer to number",
    );
}

/// Rename axis (§25): swapping T/U for Foo/Bar must keep the rule.
#[test]
fn arrow_callback_inference_independent_of_type_param_names() {
    assert_no_inference_failure(
        r#"
declare function each<Foo, Bar>(items: Foo[], cb: (item: Foo) => Bar): Bar[];
const out: string[] = each([10, 20, 30], item => item.toFixed());
"#,
        "renamed Foo/Bar type params",
    );
}

/// Function-expression form of the repro: no `this` reference in the body,
/// so it must participate in generic inference exactly like the arrow.
#[test]
fn thisless_function_expression_callback_infers_element_type() {
    assert_no_inference_failure(
        r#"
declare function map<T, U>(xs: T[], f: (x: T) => U): U[];
const r: string[] = map([1, 2, 3], function (x) { return x.toFixed(); });
"#,
        "thisless function expression",
    );
}

/// `arguments` is unrelated to `this`. The classifier must not be inverted
/// into "any reference to a special identifier means sensitive".
#[test]
fn thisless_function_expression_using_arguments_still_infers_generically() {
    assert_no_inference_failure(
        r#"
declare function reduceL<T, U>(xs: T[], f: (acc: U, x: T) => U, init: U): U;
const total: number = reduceL([1, 2, 3], function (acc, x) {
  if (arguments.length === 0) return acc;
  return acc + x;
}, 0);
"#,
        "thisless function with `arguments`",
    );
}

/// A nested arrow inherits `this` lexically, but the surrounding function
/// expression does not bind a new one. The outer function must stay thisless.
#[test]
fn nested_arrow_in_function_expression_does_not_make_outer_sensitive() {
    assert_no_inference_failure(
        r#"
declare function pickAndMap<S, R>(items: S[], transform: (item: S) => R): R[];
const result: string[] = pickAndMap([1, 2, 3], function (item) {
  const stringify = () => item.toFixed();
  return stringify();
});
"#,
        "nested-arrow-only body keeps outer function thisless",
    );
}

/// Negative axis. Guards against refactors that drop the `this`-reference
/// probe (the direction in PR #9224): a `this`-using function expression
/// must still be context-sensitive so the outer object's contextual `this`
/// type flows through Round 2 and `U` is fixed from `seed`.
#[test]
fn this_using_function_expression_remains_context_sensitive() {
    assert_no_inference_failure(
        r#"
type Config<U> = {
  seed?: () => U;
  update?: (this: { value: U }, arg: U) => void;
};
declare function build<U>(config: Config<U>): U;
const v: number = build({
  seed() { return 1; },
  update: function (arg) { this.value = arg; },
});
"#,
        "this-using function expression keeps Round-2 inference for U",
    );
}

/// Overload selection axis: thisless function expressions must let the
/// generic call pick the matching overload (here, the 3-arg pipe form).
#[test]
fn thisless_function_expression_selects_overload() {
    assert_no_inference_failure(
        r#"
declare function pipe<A, B>(f: (a: A) => B): (a: A) => B;
declare function pipe<A, B, C>(f: (a: A) => B, g: (b: B) => C): (a: A) => C;
const stages = pipe(
  function (a) { return a.toFixed(); },
  function (b) { return b.length; },
);
const out: number = stages(42);
"#,
        "overload selection with two thisless function expressions",
    );
}
