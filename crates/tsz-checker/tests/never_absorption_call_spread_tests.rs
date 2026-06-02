//! Never-absorption parity for call targets and array-literal spreads.
//!
//! `never` is the bottom type, but tsc does **not** silence every follow-up
//! operation on it. The rules verified here match `tsc` exactly:
//!
//! * Calling a `never`-typed value is always `TS2349` ("This expression is not
//!   callable. Type 'never' has no call signatures."), whether the callee is a
//!   bare identifier, an element access on `never` (`x[0]()`), or a property
//!   whose declared type is `never` (`b.value()`).
//! * A property access on a `never` *receiver* (`x.foo()`) already reports
//!   `TS2339` and collapses to the any-like error fallback, so it must **not**
//!   additionally report `TS2349`.
//! * `never` is array-like, so spreading it into an array literal (`[...x]`) is
//!   allowed and produces no `TS2488`. for-of, array-destructuring, and
//!   call-argument spreads instead route through the iterated-type path, which
//!   still reports `TS2488` for `never`.
//!
//! Binder names are varied across cases so the checks exercise structural rules
//! rather than any identifier or fixture-name fast path.

use crate::test_utils::check_source_strict_codes as check;

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Calling a `never` value: TS2349 in every form.
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
fn element_access_on_never_then_call_reports_ts2349() {
    // Element access on `never` yields `never` (no error); calling it is TS2349.
    let codes = check("function pick(empty: never) { empty[0](); }");
    assert!(
        codes.contains(&2349),
        "calling `never[0]` must report TS2349, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "element access on never must not report TS2339, got: {codes:?}"
    );
}

#[test]
fn property_whose_type_is_never_then_call_reports_ts2349() {
    // The receiver is a normal object; only the *result* of `.payload` is never,
    // so the property access itself does not error — calling it must be TS2349.
    let source = r#"
interface Wrapper<TValue> { payload: TValue; }
function run(box: Wrapper<never>) {
    box.payload();
}
"#;
    let codes = check(source);
    assert!(
        codes.contains(&2349),
        "calling a property whose type is never must report TS2349, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "an existing property of type never must not report TS2339, got: {codes:?}"
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
