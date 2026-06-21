//! Regression: the `object` keyword is assignable to an indexed-object target
//! whose string-index value type is `any` (`{ [k: string]: any }`,
//! `Record<any, any>`), matching tsc's `indexSignaturesRelatedTo` any-index
//! waiver. The waiver previously fired only for array/tuple and `{}` sources
//! (PR #14162); the `object` keyword intrinsic has no `ObjectShape`, so it never
//! reached the waiver and was wrongly rejected with TS2322/TS2344.
//!
//! Owner: `crates/tsz-solver/src/relations/subtype/core_dispatch.rs`
//! (object-keyword-source waiver, mirroring
//! `target_string_index_any_waives_missing_index`).

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn object_keyword_assignable_to_any_valued_string_index_target() {
    let codes = codes(
        r#"
declare const o: object;
const a: { [k: string]: any } = o;
const b: Record<string, any> = o;
const c: { [k: string]: any; [n: number]: any } = o;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "object should satisfy an `any`-valued string/number-index target; got {codes:?}"
    );
}

#[test]
fn generic_constrained_to_object_assignable_to_any_index_constraint() {
    // The constraint of `Value extends object` resolves to `object`, so the same
    // waiver must apply on the type-argument/constraint path (TS2344) as well.
    let codes = codes(
        r#"
type Key<Source extends { [k: string]: any }> = keyof Source;
declare function take<Value extends object>(value: Value): Key<Value>;
"#,
    );
    assert!(
        !codes.contains(&2344) && !codes.contains(&2322),
        "Value extends object should satisfy an any-valued string-index constraint; got {codes:?}"
    );
}

#[test]
fn object_keyword_still_rejects_concrete_index_value_type() {
    // Negative control: the waiver is limited to an `any` index value type. A
    // concrete `unknown` value still rejects in both tsz and tsc (object does not
    // declare a matching string index).
    let codes = codes(
        r#"
declare const o: object;
const bad: { [k: string]: unknown } = o;
"#,
    );
    assert!(
        codes.contains(&2322),
        "object is NOT assignable to a concrete `unknown`-valued index target; got {codes:?}"
    );
}

#[test]
fn object_keyword_still_rejects_required_property_target() {
    // Negative control: the `object` keyword provides no properties, so a target
    // with a *required* property still rejects even when the index value is `any`.
    let codes = codes(
        r#"
declare const o: object;
const bad: { a: number; [k: string]: any } = o;
"#,
    );
    assert!(
        codes.contains(&2322) || codes.contains(&2741),
        "object lacks the required property `a`; got {codes:?}"
    );
}
