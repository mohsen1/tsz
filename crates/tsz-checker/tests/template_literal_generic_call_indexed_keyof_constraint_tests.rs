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

/// Adjacent shape: four-level dependency, to prove the rule is not capped at
/// three levels — each `keyof Registry[..][..] & string` constraint must be
/// recognized as keying its own (deferred) indexed-access object.
#[test]
fn nested_keyof_indexed_constraint_four_levels() {
    let result = codes(
        r#"
type Registry = {
  a: { x: { p: { p1: number } } };
};
declare function f1<S extends string, T extends string, U extends string, V extends string>(
  p: `${S}.${T}.${U}.${V}`,
): void;
function f2<
  Scope extends keyof Registry & string,
  Sub extends keyof Registry[Scope] & string,
  Leaf extends keyof Registry[Scope][Sub] & string,
  Twig extends keyof Registry[Scope][Sub][Leaf] & string,
>(s: Scope, t: Sub, l: Leaf, w: Twig) {
  f1(`${s}.${t}.${l}.${w}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Adjacent shape: the `& string` key filter is what previously broke the
/// recognition. The bare `keyof Registry[Scope]` form (no intersection) must
/// keep passing, isolating the intersection idiom as the only difference.
#[test]
fn bare_keyof_indexed_constraint_without_string_filter() {
    let result = codes(
        r#"
type Registry = {
  a: { x: { x1: number }; y: { y1: number } };
  b: { z: { z1: number } };
};
declare function f1<S extends string, T extends string, U extends string>(p: `${S}.${T}.${U}`): void;
function f2<
  Scope extends keyof Registry,
  Sub extends keyof Registry[Scope],
  Leaf extends keyof Registry[Scope][Sub],
>(s: Scope, t: Sub, l: Leaf) {
  f1(`${s as string}.${t as string}.${l as string}`);
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Adjacent shape: the key filter is `& number` rather than `& string`. The
/// fix must see through any primitive key filter, not just `string`.
#[test]
fn nested_keyof_indexed_constraint_number_filter() {
    let result = codes(
        r#"
type Registry = {
  a: { 0: { x1: number }; 1: { y1: number } };
};
function read<
  Scope extends keyof Registry,
  Sub extends keyof Registry[Scope] & number,
>(reg: Registry, s: Scope, t: Sub): Registry[Scope][Sub] {
  return reg[s][t];
}
"#,
    );
    assert!(
        result.is_empty(),
        "expected no diagnostics, got: {result:?}"
    );
}

/// Negative adjacent case: a `keyof A & string` constraint used to index a
/// DIFFERENT object `B` (`B[K]`) must still report TS2536. This guards the fix
/// against over-suppression — seeing through the `& string` key filter must
/// recover the keyof *operand* `A`, and `A` is a different key space than `B`,
/// so the index stays invalid. Both the bare and `& string` forms are checked.
#[test]
fn foreign_keyof_indexed_constraint_still_reports_ts2536() {
    let result = codes(
        r#"
type A = { a1: number; a2: number };
type B = { b1: number; b2: number };
type BadBare<K extends keyof A> = B[K];
type BadFiltered<K extends keyof A & string> = B[K];
"#,
    );
    let ts2536 = result.iter().filter(|&&c| c == 2536).count();
    assert_eq!(
        ts2536, 2,
        "expected TS2536 for both `B[K]` forms keyed by a foreign object, got: {result:?}"
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
