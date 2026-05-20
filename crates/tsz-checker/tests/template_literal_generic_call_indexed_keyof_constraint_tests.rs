//! Regression coverage for #8725: template-literal generic call where one
//! type parameter's constraint involves `keyof X[Y]` (keyof of a generic
//! indexed access) must not collapse to never.
//!
//! Structural rule
//! ---------------
//! When a type parameter `K` is constrained by `keyof T[U]` (or
//! `keyof T[U] & string` / `Keyof<T[U]>` / similar) and `U` is itself a
//! type parameter constrained by `keyof T`, `keyof T[U]` is a deferred
//! generic operation. tsc does NOT evaluate it by substituting `U` with
//! its constraint (which would collapse `T[U]` to a union of T's value
//! types and then turn `keyof` into the intersection of their key sets —
//! typically `never`). The apparent type of `K` for relation purposes is
//! `string | number | symbol` (the apparent of any `keyof X`), so `K`
//! flows through a generic template literal parameter normally.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Repro from issue body (#8725 / upstream `templateLiteralTypes6.ts`).
#[test]
fn upstream_template_literal_types6_inference() {
    let result = codes(
        r#"
type Registry = {
  a: { a1: {} };
  b: { b1: {} };
};

type Keyof<T> = keyof T & string;

declare function f1<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(eventPath: `${Scope}:${Event}`): void;

function f2<
  Scope extends Keyof<Registry>,
  Event extends Keyof<Registry[Scope]>,
>(scope: Scope, event: Event) {
  f1(`${scope}:${event}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Smallest reproduction: single-span template-literal call,
/// argument is `${event}` where Event's constraint mentions
/// `keyof Registry[Scope]`.
#[test]
fn single_span_template_call_with_keyof_indexed_constraint() {
    let result = codes(
        r#"
type Registry = { a: { a1: string }; b: { b1: string } };
declare function f1<S extends string>(p: `${S}`): void;
function f2<
  Scope extends keyof Registry & string,
  Event extends keyof Registry[Scope] & string,
>(event: Event) {
  f1(`${event}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Two-span template call where both spans carry constrained type parameters,
/// the second's constraint depends on the first via `keyof T[X]`.
#[test]
fn two_span_template_call_with_keyof_indexed_constraint() {
    let result = codes(
        r#"
type Registry = { a: { a1: string }; b: { b1: string } };
declare function f1<S extends string, E extends string>(p: `${S}:${E}`): void;
function f2<
  Scope extends keyof Registry & string,
  Event extends keyof Registry[Scope] & string,
>(scope: Scope, event: Event) {
  f1(`${scope}:${event}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Rename the type parameters; the rule must be structural, not keyed on the
/// names `Scope`/`Event`/`Registry`.
#[test]
fn renamed_type_params_preserve_structural_rule() {
    let result = codes(
        r#"
type Topology = { foo: { f1: number }; bar: { b1: number } };
declare function emit<X extends string, Y extends string>(p: `${X}.${Y}`): void;
function relay<
  Domain extends keyof Topology & string,
  Leaf extends keyof Topology[Domain] & string,
>(d: Domain, l: Leaf) {
  emit(`${d}.${l}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Adjacent shape: the constraint goes through a generic alias `Keyof<T[K]>`
/// instead of inlining `keyof T[K] & string`.
#[test]
fn alias_wrapping_keyof_indexed_constraint() {
    let result = codes(
        r#"
type Registry = { a: { a1: string }; b: { b1: string } };
type Keys<T> = keyof T & string;
declare function f1<S extends string, E extends string>(p: `${S}_${E}`): void;
function f2<
  Scope extends Keys<Registry>,
  Event extends Keys<Registry[Scope]>,
>(scope: Scope, event: Event) {
  f1(`${scope}_${event}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Adjacent shape: three-level dependency. The constraint of L mentions
/// `keyof Registry[Scope][Sub]` with two layers of type-parameter indexing.
#[test]
fn nested_keyof_indexed_constraint_three_levels() {
    let result = codes(
        r#"
type Registry = {
  a: { x: { x1: number }; y: { y1: number } };
  b: { z: { z1: number } };
};
declare function f1<S extends string, T extends string, U extends string>(p: `${S}.${T}.${U}`): void;
function f2<
  Scope extends keyof Registry & string,
  Sub extends keyof Registry[Scope] & string,
  Leaf extends keyof Registry[Scope][Sub] & string,
>(s: Scope, t: Sub, l: Leaf) {
  f1(`${s}.${t}.${l}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Negative adjacent case: a literal that does NOT satisfy the prefix shape
/// must still be rejected, proving we are not silently widening keyof to
/// `string` everywhere.
#[test]
fn negative_case_template_literal_pattern_still_enforced() {
    let result = codes(
        r#"
type K = `evt_${string}`;
const bad: { [P in K]: number } = { other: 1 };
"#,
    );
    // The structural rule says `other` is not in the keyspace `evt_${string}`,
    // so TS2353 (excess property) must still fire. This test guards that the
    // broader fix did not silently widen template-literal patterns to `string`.
    assert!(
        result.contains(&2353),
        "expected TS2353 for non-matching key, got: {result:?}"
    );
}
