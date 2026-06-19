//! A homomorphic mapped type that strips optionality (`-?`) must instantiate
//! its template with the *read* type `T[K]` — which includes `| undefined`
//! for an optional source key — and only remove the resulting top-level
//! `undefined` afterwards. tsc does this through
//! `getTypeOfMappedSymbol` (`getTypeWithFacts(type, NEUndefined)`), so a
//! distributive-conditional template such as `V extends W<infer U> ? U : X`
//! sees the `undefined` member and distributes over it.
//!
//! Regression for the `propTypeValidatorInference` conformance row: tsz fed the
//! de-optionalized *declared* type into the template, so the conditional never
//! distributed over `undefined`, producing a narrower property type and a false
//! `TS2322`. Binder names are varied per case so the behavior is keyed on the
//! structural `-?` rule, not on any identifier.
use tsz_checker::test_utils::check_source_diagnostics;

fn assert_no_relevant_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    // 2318 (no global type)/2304 (cannot find name) are environment noise in the
    // minimal harness and are unrelated to the mapped-type rule under test.
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect();
    assert!(
        relevant.is_empty(),
        "expected no diagnostics, got: {relevant:#?}"
    );
}

/// The distributive `Pull<V> = V extends Box<infer U> ? U : never` over the
/// read type of an optional source key contributes `Pull<undefined>` =
/// `never`, so `Strip<{ slot?: Box<string> }>["slot"]` is `string | never` =
/// `string`. The `-?` strip then removes nothing. This is the simplest witness
/// that the template saw `Box<string> | undefined`, not `Box<string>`.
#[test]
fn strip_optional_feeds_read_type_into_distributive_template_never_branch() {
    assert_no_relevant_errors(
        r#"
declare const tag: unique symbol;
interface Box<U> { [tag]?: U }
type Pull<V> = V extends Box<infer U> ? U : never;
type Strip<M> = { [K in keyof M]-?: Pull<M[K]> };

type Source = { slot?: Box<string> };
// Strip<Source>["slot"] must be exactly `string` (never branch dropped).
const ok: string = (null as any as Strip<Source>["slot"]);
"#,
    );
}

/// When the conditional's false branch is `any` (the prop-types `InferType`
/// shape), distributing over the `undefined` member yields `any`, which makes
/// the whole property `any`. The original false positive was tsz computing the
/// narrow non-`any` type and then rejecting a structurally-wider assignment.
#[test]
fn strip_optional_distributes_undefined_to_any_false_branch() {
    assert_no_relevant_errors(
        r#"
declare const marker: unique symbol;
interface Holder<U> { [marker]?: U }
type Extract1<W> = W extends Holder<infer U> ? U : any;
type Reduce<R> = { [P in keyof R]-?: Extract1<R[P]> };

type Bag = { item?: Holder<number> };
// Extract1<Holder<number> | undefined> = number | any = any, so `item` is any
// and accepts an arbitrary value.
const accepts: { item: number } = (null as any as Reduce<Bag>);
const wide: Reduce<Bag>["item"] = "any value is fine";
"#,
    );
}

/// `-?` must still strip the top-level `undefined` an optional key contributes
/// for an identity template, so `Required`-style mappings keep producing the
/// de-optionalized property type (no spurious `| undefined`).
#[test]
fn strip_optional_removes_top_level_undefined_for_identity_template() {
    let diagnostics = check_source_diagnostics(
        r#"
type Demand<T> = { [K in keyof T]-?: T[K] };
type Loose = { field?: string };
// Demand<Loose>["field"] is `string`, NOT `string | undefined`.
const bad: undefined = (null as any as Demand<Loose>["field"]);
"#,
    );
    // Exactly one TS2322: `string` is not assignable to `undefined`.
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 (string not assignable to undefined), got: {diagnostics:#?}"
    );
}

/// The strip is top-level only: a `-?` template whose result nests `undefined`
/// inside an object keeps it, matching tsc's `getTypeWithFacts` (which only
/// removes the union-level `undefined`).
#[test]
fn strip_optional_keeps_nested_undefined() {
    let diagnostics = check_source_diagnostics(
        r#"
type Wrap<T> = { [K in keyof T]-?: { value: T[K] } };
type Src = { entry?: string };
// Wrap<Src>["entry"] is `{ value: string | undefined }`; nested undefined kept.
const bad: { value: string } = (null as any as Wrap<Src>["entry"]);
"#,
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 (nested undefined kept), got: {diagnostics:#?}"
    );
}
