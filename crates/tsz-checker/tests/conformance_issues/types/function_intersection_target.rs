//! Regression coverage for assigning a function value to an intersection
//! target whose members combine an object type with a call signature
//! (`Meta & ((...args) => Fn)`). Tracks issue #14156 (remeda
//! `purryFromLazy.ts`).
//!
//! Structural rule under test:
//!
//! > A source is assignable to `A & B` iff it is assignable to each of `A`
//! > and `B` independently. A function value is an object whose apparent
//! > type carries the global `Function` interface members, so it satisfies
//! > an all-optional object constituent (`{ x?: T }`) while still being
//! > rejected by an object constituent with a required member it lacks.
//!
//! Every test varies binder names so a fixture-name fast path cannot pass
//! these checks, and exercises `satisfies`, plain assignment, and parameter
//! passing so they agree. Positive cases (all-optional object constituent)
//! must type-check; negative controls (required member the function lacks)
//! must still report TS1360/TS2322/TS2345.

use super::super::core::*;

fn no_assignability_errors(source: &str) {
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        !has_error(&diagnostics, 1360)
            && !has_error(&diagnostics, 2322)
            && !has_error(&diagnostics, 2345),
        "expected no assignability errors. diagnostics: {diagnostics:#?}\nsource:\n{source}"
    );
}

#[test]
fn satisfies_function_against_object_and_call_intersection_property() {
    // The reduced remeda repro: the target property `lazy` has type
    // `LazyMeta & ((...args: any) => LazyFn)`; the source `lazy` is a bare
    // function whose return type is covariant-compatible with `LazyFn`.
    no_assignability_errors(
        r#"
type LazyResult<R> = { done: boolean; next: R };
type LazyEvaluator<T = unknown, R = unknown> = (
  item: T,
  index: number,
  data: readonly T[],
) => LazyResult<R>;
type LazyFn = (
  value: unknown,
  index: number,
  items: readonly unknown[],
) => LazyResult<unknown>;
type LazyMeta = { readonly single?: boolean };
type LazyDefinition = {
  readonly lazy: LazyMeta & ((...args: any) => LazyFn);
  readonly lazyArgs: readonly unknown[];
};

export function make(
  lazy: (...args: any) => LazyEvaluator,
  args: readonly unknown[],
) {
  const [, ...rest] = args;
  return { lazy, lazyArgs: rest } satisfies LazyDefinition;
}
"#,
    );
}

#[test]
fn satisfies_function_against_top_level_object_and_call_intersection() {
    // Top-level form (not nested in an object literal target).
    no_assignability_errors(
        r#"
type Meta = { readonly single?: boolean };
type Call = (...args: any) => number;

declare const fn: (...args: any) => number;
const ok = fn satisfies Meta & Call;
"#,
    );
}

#[test]
fn assignment_function_against_object_and_call_intersection_renamed_binders() {
    // Renamed binders (anti-hardcoding) + plain assignment form.
    no_assignability_errors(
        r#"
type Brand = { readonly tag?: string };
type Handler = (input: unknown) => boolean;

declare const probe: (input: unknown) => true;
const slot: Brand & Handler = probe;
"#,
    );
}

#[test]
fn parameter_passing_function_against_object_and_call_intersection() {
    // Parameter-passing form must agree with `satisfies` / assignment.
    no_assignability_errors(
        r#"
type Marker = { readonly hidden?: boolean };
type Fn = (x: unknown) => void;

declare function accept(value: Marker & Fn): void;
declare const cb: (x: unknown) => void;
accept(cb);
"#,
    );
}

#[test]
fn function_against_intersection_with_required_member_is_rejected() {
    // Negative control: the object constituent has a *required* member that
    // a bare function does not carry, so the relation must fail (TS2322),
    // matching tsc.
    let source = r#"
type RequiredMeta = { tag: string };
type Fn = (x: unknown) => void;

declare const cb: (x: unknown) => void;
const slot: RequiredMeta & Fn = cb;
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        has_error(&diagnostics, 2322),
        "expected TS2322 because the function lacks the required `tag`. diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn function_against_weak_object_alone_is_rejected() {
    // Negative control: a *direct* assignment to a weak (all-optional) object
    // type — not an intersection member — still reports the weak-type error
    // (TS2559), because the function has no property in common with it.
    let source = r#"
type Weak = { readonly single?: boolean };
declare const cb: (...args: any) => void;
const slot: Weak = cb;
"#;
    let diagnostics = compile_and_get_diagnostics_with_lib(source);
    assert!(
        has_error(&diagnostics, 2559) || has_error(&diagnostics, 2322),
        "expected a weak-type / assignability error for a bare function vs a weak object. diagnostics: {diagnostics:#?}"
    );
}
