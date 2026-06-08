//! Regression tests for assignability of a bare type parameter `T` to a
//! homomorphic mapped type over a *key subset* of `keyof T` — the `Pick<T, K>`
//! shape `{ [P in K]: T[P] }`.
//!
//! `tsc` accepts `T <: Pick<T, SomeKeys<T>>` whenever `SomeKeys<T>` is a subset
//! of `keyof T`: every demanded key `P` is a key of `T`, so `T` supplies each
//! property with a matching type. tsz previously only recognized the *full*
//! `keyof T` case (`Pick<T, keyof T>`) and emitted a spurious `TS2322` for any
//! computed subset (`Pick<T, FunctionPropertyNames<T>>`, `Pick<T, keyof T & string>`,
//! …). This is the false positive that lined `conditionalTypes1.ts` on the
//! accepted-regression ledger (the `f7` block: `y = x` / `z = x`).
//!
//! The rule must stay sound in the other direction: a `Pick<T, A>` subset is NOT
//! assignable back to `T` (it is missing keys), nor to a `Pick<T, B>` with a
//! different key subset.

use tsz_checker::test_utils::check_source_diagnostics;

/// Count of `TS2322` (type-not-assignable) diagnostics in `source`.
fn ts2322_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|diag| diag.code == 2322)
        .count()
}

#[test]
fn type_param_assignable_to_pick_over_computed_subset() {
    // `StringKeys<T>` is a proper subset of `keyof T`; `T` carries every such
    // property, so `T <: Pick<T, StringKeys<T>>`. No lib types are referenced so
    // the fixture stays hermetic.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type StringKeys<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];

function f<T>(x: T, y: Pick<T, StringKeys<T>>) {
    y = x;
}
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "T should be assignable to Pick<T, StringKeys<T>> (subset of keyof T)"
    );
}

#[test]
fn type_param_assignable_to_intersection_key_subset() {
    // `keyof T & string` is a subset of `keyof T`.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

function f<T>(x: T, y: Pick<T, keyof T & string>) {
    y = x;
}
"#;
    assert_eq!(ts2322_count(source), 0);
}

#[test]
fn type_param_assignable_to_inline_subset_mapped() {
    // The same shape spelled inline (not via a `Pick` alias).
    let source = r#"
type StringKeys<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];

function f<T>(x: T, y: { [P in StringKeys<T>]: T[P] }) {
    y = x;
}
"#;
    assert_eq!(ts2322_count(source), 0);
}

#[test]
fn rule_is_structural_not_keyed_on_binder_names() {
    // Renaming every binder (type parameter and key parameter) must not change
    // the outcome: the rule is structural, never name-driven.
    let source = r#"
type Subset<Elem, Prop extends keyof Elem> = { [Each in Prop]: Elem[Each] };
type TextKeys<Elem> = { [Each in keyof Elem]: Elem[Each] extends string ? Each : never }[keyof Elem];

function widget<Elem>(whole: Elem, part: Subset<Elem, TextKeys<Elem>>) {
    part = whole;
}
"#;
    assert_eq!(ts2322_count(source), 0);
}

#[test]
fn pick_subset_not_assignable_back_to_type_param() {
    // Sound direction: the subset `Pick<T, K>` is missing T's other keys, so it
    // is NOT assignable to `T`.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type StringKeys<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];

function f<T>(x: T, y: Pick<T, StringKeys<T>>) {
    x = y;
}
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "Pick<T, subset> must not be assignable back to the whole T"
    );
}

#[test]
fn distinct_key_subsets_are_not_mutually_assignable() {
    // Two Picks over disjoint computed key subsets are not interchangeable.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type StringKeys<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];
type NumberKeys<T> = { [K in keyof T]: T[K] extends number ? K : never }[keyof T];

function f<T>(y: Pick<T, StringKeys<T>>, z: Pick<T, NumberKeys<T>>) {
    y = z;
}
"#;
    assert_eq!(ts2322_count(source), 1);
}

#[test]
fn full_keyof_pick_round_trips_in_both_assignment_directions() {
    // The pre-existing full-`keyof` case keeps working: `T` and `Pick<T, keyof T>`
    // have the same key set, so assignment succeeds both ways.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

function f<T>(x: T, y: Pick<T, keyof T>) {
    y = x;
    x = y;
}
"#;
    assert_eq!(ts2322_count(source), 0);
}

#[test]
fn function_and_non_function_property_subsets_match_tsc() {
    // The exact `conditionalTypes1.ts` `f7` witness, spelled with a `string`-based
    // discriminator instead of the lib `Function` type so the fixture is
    // hermetic. `y = x` and `z = x` (T into a Pick subset) are clean; the four
    // cross-direction / cross-subset assignments stay errors.
    let source = r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type TextNames<T> = { [K in keyof T]: T[K] extends string ? K : never }[keyof T];
type OtherNames<T> = { [K in keyof T]: T[K] extends string ? never : K }[keyof T];
type TextProperties<T> = Pick<T, TextNames<T>>;
type OtherProperties<T> = Pick<T, OtherNames<T>>;

function f<T>(x: T, y: TextProperties<T>, z: OtherProperties<T>) {
    x = y;  // error: subset not assignable to whole
    x = z;  // error: subset not assignable to whole
    y = x;  // ok: T assignable to its text-key subset
    y = z;  // error: disjoint subsets
    z = x;  // ok: T assignable to its other-key subset
    z = y;  // error: disjoint subsets
}
"#;
    assert_eq!(
        ts2322_count(source),
        4,
        "only the four cross-direction / cross-subset assignments should error"
    );
}
