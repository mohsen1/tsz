//! Never-absorption parity for array-literal spreads (and guards for `never`
//! call/property behavior that must stay matched to `tsc`).
//!
//! The headline fix here: `never` is array-like (`never <: readonly any[]`), so
//! spreading it into an **array literal** (`[...x]`) is allowed and produces a
//! `never[]` element with no `TS2488`. The for-of, array-destructuring, and
//! call-argument spread paths instead route through the iterated-type check,
//! which still reports `TS2488` for `never` — so the exemption must stay scoped
//! to array-literal value spreads.
//!
//! The remaining cases are regression guards for `never` call/property behavior
//! that already matches `tsc`: calling a bare `never` identifier is `TS2349`,
//! while property/private-name access on a `never` *receiver* reports `TS2339`
//! (once, no cascade) and the trailing call adds no redundant `TS2349`.
//!
//! Binder names are varied across cases so the checks exercise structural rules
//! rather than any identifier or fixture-name fast path.

use crate::test_utils::check_source_strict_codes as check;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Calling / accessing a `never` value.
// ---------------------------------------------------------------------------

#[test]
fn direct_never_call_reports_ts2349() {
    let codes = check("function consume(bottom: never) { bottom(); }");
    assert_eq!(
        count(&codes, 2349),
        1,
        "calling a never-typed identifier must report exactly one TS2349, got: {codes:?}"
    );
}

#[test]
fn property_access_on_never_receiver_then_call_is_single_ts2339() {
    // Receiver is never: `holder.member` reports TS2339 and becomes the any-like
    // fallback, so the trailing call must NOT add a redundant TS2349.
    let codes = check("function reach(holder: never) { holder.member(); }");
    assert_eq!(
        count(&codes, 2339),
        1,
        "property access on a never receiver should report exactly one TS2339, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2349),
        "property access on a never receiver must not also report TS2349, got: {codes:?}"
    );
}

#[test]
fn property_access_chain_on_never_receiver_does_not_cascade() {
    // `.first` errors once; the any-like fallback absorbs `.second`.
    let codes = check("function deep(origin: never) { origin.first.second; }");
    assert_eq!(
        count(&codes, 2339),
        1,
        "a property chain on a never receiver should report TS2339 once, got: {codes:?}"
    );
}

#[test]
fn private_name_access_and_call_on_never_receiver_is_only_ts2339() {
    // Private-name access on a `never` receiver reports TS2339 per access and the
    // trailing call adds no TS2349.
    let source = r#"
class Holder {
    #field = 0;
    #method() {}
    reach(empty: never) {
        empty.#field;
        empty.#method();
        empty.#field();
    }
}
"#;
    let codes = check(source);
    assert_eq!(
        count(&codes, 2339),
        3,
        "private-name access on never should report TS2339 per access, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2349),
        "private-name calls on a never receiver must not report TS2349, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Calling a `never` callee that has NO companion TS2339.
//
// A property access on a `never` *receiver* collapses to the any-like error
// fallback (TS2339, no TS2349 — covered above). But a callee whose type is
// `never` for any other reason still has no call signatures, so `tsc` reports
// TS2349: indexed access on a `never` receiver (silent access), a member that
// legitimately resolves to a `never`-typed value, an index signature whose
// value is `never`, and a `never[]` element. These previously slipped through
// because the suppression keyed on the *syntactic* member-access shape rather
// than on whether a companion diagnostic actually fired.
// ---------------------------------------------------------------------------

#[test]
fn indexed_access_on_never_receiver_call_reports_ts2349() {
    // `empty["slot"]` is `never` and emits no TS2339 (indexed access on never is
    // silent), so the trailing call must still report TS2349.
    let codes = check(r#"function reach(empty: never) { empty["slot"](); }"#);
    assert_eq!(
        count(&codes, 2349),
        1,
        "indexed access on a never receiver then call should report one TS2349, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "indexed access on a never receiver is silent — no TS2339, got: {codes:?}"
    );
}

#[test]
fn never_typed_member_call_reports_ts2349() {
    // The member exists (no TS2339) but is typed `never`, so both the dotted and
    // the indexed call have no call signatures.
    let dotted = check("function take(holder: { slot: never }) { holder.slot(); }");
    assert_eq!(
        count(&dotted, 2349),
        1,
        "dotted call on a never-typed member should report one TS2349, got: {dotted:?}"
    );
    assert!(
        !dotted.contains(&2339),
        "the member exists — no TS2339, got: {dotted:?}"
    );

    let indexed = check(r#"function grab(carrier: { slot: never }) { carrier["slot"](); }"#);
    assert_eq!(
        count(&indexed, 2349),
        1,
        "indexed call on a never-typed member should report one TS2349, got: {indexed:?}"
    );
}

#[test]
fn never_index_signature_value_call_reports_ts2349() {
    // A string index signature whose value type is `never` means every element
    // is `never`.
    let dotted = check("function load(table: { [k: string]: never }) { table.key(); }");
    assert_eq!(
        count(&dotted, 2349),
        1,
        "dotted call through a never index signature should report one TS2349, got: {dotted:?}"
    );

    let indexed =
        check(r#"function fetch(registry: { [k: string]: never }) { registry["key"](); }"#);
    assert_eq!(
        count(&indexed, 2349),
        1,
        "indexed call through a never index signature should report one TS2349, got: {indexed:?}"
    );
}

#[test]
fn never_array_element_call_reports_ts2349() {
    let codes = check("function scan(list: never[]) { list[0](); }");
    assert_eq!(
        count(&codes, 2349),
        1,
        "calling a never[] element should report one TS2349, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Optional call (`?.()`) on an entirely-nullish callee.
//
// When the callee of an optional call is exactly `null`/`undefined` (or their
// union), the chain short-circuits to `undefined`, but `tsc` computes the
// non-nullish slice as `never` (no call signatures) and reports TS2349. A
// callee that still has a callable non-nullish slice is fine.
// ---------------------------------------------------------------------------

#[test]
fn optional_call_on_undefined_callee_reports_ts2349() {
    let codes = check("function ring(buzzer: undefined) { buzzer?.(); }");
    assert_eq!(
        count(&codes, 2349),
        1,
        "optional call on an `undefined` callee should report one TS2349, got: {codes:?}"
    );
}

#[test]
fn optional_call_on_nullish_union_callee_reports_ts2349() {
    let codes = check("function ping(beacon: null | undefined) { beacon?.(); }");
    assert_eq!(
        count(&codes, 2349),
        1,
        "optional call on a `null | undefined` callee should report one TS2349, got: {codes:?}"
    );
}

#[test]
fn optional_call_on_callable_or_undefined_is_allowed() {
    // The non-nullish slice is callable, so the optional call is valid.
    let codes = check("function maybe(handler: (() => void) | undefined) { handler?.(); }");
    assert!(
        !codes.contains(&2349),
        "optional call with a callable non-nullish slice must not report TS2349, got: {codes:?}"
    );
}

#[test]
fn optional_call_on_present_method_is_allowed() {
    let codes = check("function wire(panel: { run?: () => void }) { panel.run?.(); }");
    assert!(
        !codes.contains(&2349),
        "optional call on a present optional method must not report TS2349, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Spreading `never`: allowed in array literals, rejected elsewhere.
// ---------------------------------------------------------------------------

#[test]
fn array_literal_spread_of_never_is_allowed() {
    let codes = check("function build(items: never) { const collected = [...items]; }");
    assert!(
        !codes.contains(&2488),
        "spreading never into an array literal must not report TS2488, got: {codes:?}"
    );
}

#[test]
fn array_literal_spread_of_empty_intersection_is_allowed() {
    // `string & number` reduces to never, which is array-like.
    let codes = check("function build(value: string & number) { const out = [...value]; }");
    assert!(
        !codes.contains(&2488),
        "spreading an empty intersection (never) into an array literal must not report TS2488, got: {codes:?}"
    );
}

#[test]
fn array_literal_spread_of_never_in_middle_is_allowed() {
    let codes = check("function build(gap: never) { const out = [1, ...gap, 2]; }");
    assert!(
        !codes.contains(&2488),
        "spreading never between elements must not report TS2488, got: {codes:?}"
    );
}

#[test]
fn for_of_over_never_still_reports_ts2488() {
    let codes = check("function loop(stream: never) { for (const item of stream) { item; } }");
    assert!(
        codes.contains(&2488),
        "for-of over never must still report TS2488, got: {codes:?}"
    );
}

#[test]
fn array_destructuring_of_never_still_reports_ts2488() {
    let codes = check("function take(tuple: never) { const [head] = tuple; head; }");
    assert!(
        codes.contains(&2488),
        "array-destructuring of never must still report TS2488, got: {codes:?}"
    );
}

#[test]
fn call_argument_spread_of_never_still_reports_ts2488() {
    let source = r#"
declare function sink(...rest: unknown[]): void;
function forward(args: never) {
    sink(...args);
}
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2488),
        "call-argument spread of never must still report TS2488, got: {codes:?}"
    );
}
