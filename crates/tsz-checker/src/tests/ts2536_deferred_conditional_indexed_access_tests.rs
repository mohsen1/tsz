//! Regression coverage for TS2536 on *nested* indexed accesses into a deferred
//! conditional type: `Cond<T>[k1][k2]`.
//!
//! The inner `Cond<T>[k1]` is a generic indexed access whose apparent type is
//! the conditional's branch-union constraint indexed by `k1` (tsc's
//! `getConstraintOfIndexedAccessType`). That apparent type carries a concrete
//! key space even while the access stays deferred, so the outer literal key `k2`
//! is validated against it: keys present in the apparent type (e.g. `length`,
//! numeric indices, array methods on a tuple-valued part) are accepted while a
//! genuinely-missing key still emits TS2536. Mirrors tsc, which keeps the access
//! deferred but never collapses the apparent key space to empty.
//!
//! Binder names vary across cases so no fixture/identifier string drives the
//! decision.

use crate::test_utils::check_source_codes;

/// `Cond<T>["required"]["length"]` — `length` is in the tuple apparent type of
/// the `required` part, so no TS2536. Matches tsc.
#[test]
fn nested_deferred_conditional_length_index_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Parts<T extends ReadonlyArray<unknown>, Prefix extends unknown[] = []> =
  T extends readonly [infer Head, ...infer Tail]
    ? Parts<Tail, [...Prefix, Head]>
    : { required: Prefix; optional: []; suffix: [] };
type Len<T extends ReadonlyArray<unknown>> = Parts<T>["required"]["length"];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for `Parts<T>[\"required\"][\"length\"]`: {codes:?}"
    );
}

/// Numeric index into the tuple apparent type — also valid.
#[test]
fn nested_deferred_conditional_numeric_index_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type Cond<U extends ReadonlyArray<unknown>> = U extends readonly []
  ? { head: [1] }
  : { head: [2] };
type First<U extends ReadonlyArray<unknown>> = Cond<U>["head"][0];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for numeric index `Cond<U>[\"head\"][0]`: {codes:?}"
    );
}

/// Named branch aliases still contribute their tuple members to the apparent
/// key space; raw resolverless `keyof` used to see the `Lazy(DefId)` branches as
/// unresolved and reject the outer `length` key.
#[test]
fn nested_aliased_branch_deferred_conditional_length_index_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type PresentPart = { bucket: [1, 2] };
type FallbackPart = { bucket: [3] };
type Branches<Item> = Item extends string ? PresentPart : FallbackPart;
type Len<Item> = Branches<Item>["bucket"]["length"];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for aliased branch tuple `length`: {codes:?}"
    );
}

/// Inline deferred conditional (no alias indirection) — same acceptance for an
/// array method key.
#[test]
fn inline_nested_deferred_conditional_method_index_does_not_emit_ts2536() {
    let codes = check_source_codes(
        r#"
type M<V> = (V extends string ? { a: [1] } : { a: [2] })["a"]["map"];
"#,
    );
    assert!(
        !codes.contains(&2536),
        "TS2536 should not fire for `(...)[\"a\"][\"map\"]` array-method index: {codes:?}"
    );
}

/// Negative: a key absent from the apparent type STILL emits TS2536 — the
/// suppression must not be a blanket defer.
#[test]
fn nested_deferred_conditional_bogus_key_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Cond<W extends ReadonlyArray<unknown>> = W extends readonly []
  ? { part: [1] }
  : { part: [2] };
type Bad<W extends ReadonlyArray<unknown>> = Cond<W>["part"]["nope"];
"#,
    );
    assert!(
        codes.contains(&2536),
        "TS2536 expected for missing key `Cond<W>[\"part\"][\"nope\"]`: {codes:?}"
    );
}

/// Negative: a first-level key absent from every branch still emits TS2536
/// (unchanged behavior — guards the inner access too).
#[test]
fn deferred_conditional_first_level_missing_key_emits_ts2536() {
    let codes = check_source_codes(
        r#"
type Cond<X extends ReadonlyArray<unknown>> = X extends readonly []
  ? { only: [1] }
  : { only: [2] };
type Bad<X extends ReadonlyArray<unknown>> = Cond<X>["missing"];
"#,
    );
    assert!(
        codes.contains(&2536),
        "TS2536 expected for first-level missing key `Cond<X>[\"missing\"]`: {codes:?}"
    );
}
