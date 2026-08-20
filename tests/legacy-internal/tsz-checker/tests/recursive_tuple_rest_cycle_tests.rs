//! Regression tests for recursive generic interfaces with tuple-rest members.
//!
//! Structural rule: when a self-recursive generic interface has a member tuple
//! containing a concrete array rest (e.g. `items: [Self<T>, ...number[]]`),
//! the evaluator's tuple rebuild must keep the rest element's `type_id` in its
//! spread-operand (array) form. Before the fix, the rebuild unwrapped
//! `...number[]` to `number`, so `check_tuple_subtype` compared `number`
//! against `number[]` and rejected relations tsc accepts — surfacing as a
//! false `TS2344` on the constraint path and a false `TS2322` on the
//! assignability path (valibot `BaseIssue<TInput>.issues` family).
//!
//! Per the anti-hardcoding gate, binder names vary across cases so the tests
//! pin the structural rule, not a spelling.

use crate::test_utils::check_source_diagnostics;

fn codes(src: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_diagnostics(src)
        .iter()
        .map(|d| d.code)
        .collect();
    v.sort_unstable();
    v
}

// ── Witness: constraint-satisfaction path (TS2344) ──────────────────────────

#[test]
fn recursive_tuple_rest_constraint_path_accepts_narrower_arg() {
    // tsc: clean. The cycle on `Box<string> <: Box<unknown>` is assumed
    // related; the `...Box<T>[]` rest compares array-to-array.
    assert_eq!(
        codes(
            r#"
interface Box<T> {
  items: [Box<T>, ...Box<T>[]];
}
type Use<X extends Box<unknown>> = X;
type P = Use<Box<string>>;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Witness: plain assignability path (TS2322) ──────────────────────────────

#[test]
fn recursive_tuple_rest_assignability_accepts_narrower_arg() {
    assert_eq!(
        codes(
            r#"
interface Node2<T> {
  kids: [Node2<T>, ...Node2<T>[]];
}
declare const ns: Node2<string>;
const nu: Node2<unknown> = ns;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Concrete array rest beside a recursive fixed element ────────────────────

#[test]
fn recursive_fixed_element_with_concrete_array_rest_accepted() {
    // The rest array's element is concrete (no type parameter); the fixed
    // element re-enters the in-progress relation. tsc: clean.
    assert_eq!(
        codes(
            r#"
interface Chain<T> {
  links: [Chain<T>, ...number[]];
}
declare const cs: Chain<string>;
const cu: Chain<unknown> = cs;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Valibot shape: optional + readonly + `| undefined` wrapper ──────────────

#[test]
fn valibot_shaped_optional_readonly_tuple_rest_accepted() {
    assert_eq!(
        codes(
            r#"
interface Issue<T> {
  readonly kind: string;
  readonly issues?: [Issue<T>, ...Issue<T>[]] | undefined;
}
type Msg<X extends Issue<unknown>> = X;
type P = Msg<Issue<string>>;
declare const i: Issue<string>;
const j: Issue<unknown> = i;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── NEGATIVE control: tsc also rejects (must stay rejected) ─────────────────

#[test]
fn directly_witnessed_param_mismatch_still_rejected() {
    // `value: T` pins the variance, so `Crate<string>` is NOT assignable to
    // `Crate<number>` (tsc: TS2322), while `Crate<unknown>` stays accepted.
    let src_fail = r#"
interface Crate<T> { value: T; parts: [Crate<T>, ...number[]]; }
declare const cs: Crate<string>;
const cn: Crate<number> = cs;
"#;
    assert_eq!(codes(src_fail), vec![2322]);

    let src_ok = r#"
interface Crate<T> { value: T; parts: [Crate<T>, ...number[]]; }
declare const cs: Crate<string>;
const cu: Crate<unknown> = cs;
"#;
    assert_eq!(codes(src_ok), Vec::<u32>::new());
}

// ── Alias-wrapped constraint ────────────────────────────────────────────────

#[test]
fn alias_wrapped_recursive_tuple_rest_constraint_accepted() {
    assert_eq!(
        codes(
            r#"
interface Wrap<T> {
  parts: [Wrap<T>, ...string[]];
}
type Alias<T> = Wrap<T>;
type Need<X extends Alias<unknown>> = X;
type Q = Need<Alias<boolean>>;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Controls: tuple without rest / rest-only array form ─────────────────────

#[test]
fn closed_tuple_without_rest_control_accepted() {
    assert_eq!(
        codes(
            r#"
interface Pair<T> { items: [Pair<T>, Pair<T>]; }
declare const ps: Pair<string>;
const pu: Pair<unknown> = ps;
"#
        ),
        Vec::<u32>::new()
    );
}

#[test]
fn rest_only_tuple_control_accepted() {
    assert_eq!(
        codes(
            r#"
interface Many<T> { items: [...Many<T>[]]; }
declare const ms: Many<string>;
const mu: Many<unknown> = ms;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Union-of-arrays rest (evaluator union distribution arm) ─────────────────

#[test]
fn union_array_rest_beside_recursive_element_accepted() {
    // tsc: clean. Exercises the union-spread distribution arm of the tuple
    // evaluator, which must also keep array members in spread-operand form.
    assert_eq!(
        codes(
            r#"
type Mix = number[] | string[];
interface Pack<T> { entries: [Pack<T>, ...Mix]; }
declare const ps: Pack<string>;
const pu: Pack<unknown> = ps;
"#
        ),
        Vec::<u32>::new()
    );
}

// ── Nested application argument ─────────────────────────────────────────────

#[test]
fn nested_application_argument_accepted() {
    assert_eq!(
        codes(
            r#"
interface Deep<T> {
  items: [Deep<T>, ...Deep<T>[]];
}
type Use<X extends Deep<unknown>> = X;
type R = Use<Deep<Deep<string>>>;
"#
        ),
        Vec::<u32>::new()
    );
}
