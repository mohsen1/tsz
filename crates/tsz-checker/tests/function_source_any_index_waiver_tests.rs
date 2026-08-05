//! Regression: a function-like source is assignable to an indexed-object target
//! whose string-index value type is `any` (`{ [k: string]: any }`,
//! `Record<string, any>`), matching tsc's `indexSignaturesRelatedTo` any-index
//! waiver.
//!
//! The waiver already fired for object, array/tuple, `{}` and `object`-keyword
//! sources, but a bare `FunctionShape` never reached it: the callable arm of
//! `core_dispatch` returned an unconditional `False` for any indexed target it
//! could not satisfy structurally. Every row below was a TS2322 false positive.
//!
//! The rule tsc implements (checker.ts `indexSignaturesRelatedTo`) is
//! `relation != strictSubtype && !sourceIsPrimitive && targetHasStringIndex &&
//! targetInfo.type is any` short-circuiting *each* index info of the target — so
//! a co-present `any`-valued number index is waived too, while a concrete index
//! value type (`unknown`, `string`) and a primitive source never are. All rows
//! here are pinned against `typescript@7.0.2`.
//!
//! Owner: `crates/tsz-solver/src/relations/subtype/core_dispatch.rs`
//! (function-like-source arms, delegating to the shared
//! `target_string_index_any_waives_missing_index` /
//! `target_dual_any_index_waives_missing_number_index` helpers).

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn function_sources_assignable_to_any_valued_string_index_target() {
    // Every function-like source form: arrow, function expression, named
    // function expression, hoisted declaration, and one with parameters. The
    // binder names vary deliberately — none of them may drive the verdict.
    let codes = codes(
        r#"
type StringToAny = { [k: string]: any };
function hoisted() {}
const a: StringToAny = () => {};
const b: StringToAny = function () {};
const c: StringToAny = hoisted;
const d: StringToAny = function namedInner() { return 1; };
const e: StringToAny = (first: number, second: string) => first;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "a function source should satisfy an `any`-valued string-index target; got {codes:?}"
    );
}

#[test]
fn function_source_assignable_to_inline_and_aliased_any_index_targets() {
    // Alias, generic alias instantiated at `any`, and the inline literal must
    // agree — the waiver is a property of the resolved index value type, not of
    // how the target was written.
    let codes = codes(
        r#"
type StringMap<Value> = { [k: string]: Value };
type Rec = { [k: string]: any };
const inline: { [k: string]: any } = () => {};
const aliased: Rec = () => {};
const viaGeneric: StringMap<any> = () => {};
const readonlyIndex: { readonly [k: string]: any } = () => {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "alias/generic/readonly spellings of an any-valued string index should all waive; got {codes:?}"
    );
}

#[test]
fn function_source_waives_co_present_any_valued_number_index() {
    // tsc short-circuits *every* index info of a target that has an
    // `any`-valued string index, so the number index is waived as well —
    // even though a number-index-only target rejects the same source.
    let codes = codes(
        r#"
const both: { [k: string]: any; [n: number]: any } = () => {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "an any-valued string index waives a co-present any-valued number index; got {codes:?}"
    );
}

#[test]
fn function_source_still_rejects_number_index_only_target() {
    // Negative control, and the exact boundary of the previous test: without a
    // string index to trigger the short-circuit, a function supplies no numeric
    // index and tsc rejects.
    let codes = codes(
        r#"
const numberOnly: { [n: number]: any } = () => {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "a function supplies no numeric index; a number-index-only target must reject; got {codes:?}"
    );
}

#[test]
fn function_source_still_rejects_number_index_with_concrete_value_type() {
    // The waiver is per-index-info: the `any` string index passes, but a number
    // index whose value type is *not* `any` still demands a real numeric index.
    let codes = codes(
        r#"
const mixed: { [k: string]: any; [n: number]: string } = () => {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "a non-any number index is not waived by an any string index; got {codes:?}"
    );
}

#[test]
fn function_source_still_rejects_concrete_string_index_value_type() {
    // Negative controls on the string side. `unknown` is the trap: it looks like
    // a supertype of everything, but tsc waives only on exactly `any`.
    let codes = codes(
        r#"
type StringMap<Value> = { [k: string]: Value };
const toUnknown: { [k: string]: unknown } = () => {};
const toString2: { [k: string]: string } = () => {};
const viaGenericUnknown: StringMap<unknown> = () => {};
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        3,
        "only `any` is waived; `unknown` and `string` index value types must all reject; got {codes:?}"
    );
}

#[test]
fn any_index_waiver_does_not_excuse_a_missing_required_property() {
    // The waiver covers the *index* obligation only. A required property the
    // function's apparent type does not carry still fails, exactly as tsc
    // reports it — the fix must not degrade into a blanket accept.
    let codes = codes(
        r#"
const missingMember: { zzz: string; [k: string]: any } = () => {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "a required property a function does not have must still reject; got {codes:?}"
    );
}

#[test]
fn any_index_waiver_does_not_excuse_a_construct_signature() {
    // Signature obligations survive the waiver too: a plain arrow is not newable.
    let codes = codes(
        r#"
const needsNew: { new (): any; [k: string]: any } = () => {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "a non-newable source must still fail a construct-signature target; got {codes:?}"
    );
}

#[test]
fn any_index_waiver_accepts_optional_only_members_alongside_the_index() {
    // Mirror of the required-property control: optional members impose no
    // obligation, so the waived target is satisfied.
    let codes = codes(
        r#"
const optionalOnly: { zzz?: string; [k: string]: any } = () => {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "optional members impose no obligation on a waived target; got {codes:?}"
    );
}

#[test]
fn primitive_sources_never_reach_the_any_index_waiver() {
    // tsc's short-circuit is guarded by `!sourceIsPrimitive`. These are not
    // function sources, but they share the waived target and must stay rejected,
    // pinning that the fix did not widen the gate to every source kind.
    let codes = codes(
        r#"
const fromNumber: { [k: string]: any } = 1;
const fromString: { [k: string]: any } = "s";
const fromBoolean: { [k: string]: any } = true;
"#,
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        3,
        "primitive sources are excluded from the any-index waiver; got {codes:?}"
    );
}

#[test]
fn function_source_satisfies_a_call_member_alongside_the_any_index() {
    // The pre-existing `call`/`apply` compatibility bridge and the waiver must
    // compose rather than shadow one another.
    let codes = codes(
        r#"
const withCall: { call(...a: any[]): any; [k: string]: any } = () => {};
const withCallSig: { (): void; [k: string]: any } = () => {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "a call member/signature alongside a waived index must still accept; got {codes:?}"
    );
}
