//! Regression tests for the structural flow-cache key used by property and
//! element reference narrowing.
//!
//! Property/element references (`x.a`, `x["a"]`) do not resolve to a single
//! `SymbolId`, so the flow analyzer keys their narrowing cache on a structural
//! reference *path* — `[base_symbol_id, prop_atom, ...]` — interned to a
//! synthetic cache symbol. This lets every syntactic occurrence of the same
//! path share cache entries (turning the previous O(N²) per-occurrence flow
//! re-walk into O(N)).
//!
//! Correctness hinges on the key never aliasing two *different* references.
//! These tests would fail if the structural key dropped the base symbol (so
//! `a.v` and `b.v` collided), dropped the property (so `o.a` and `o.b`
//! collided), or corrupted the narrowed type across repeated reads of the same
//! path.

use tsz_checker::test_utils::check_source_strict_codes;

/// Two distinct bases (`first` vs `second`) that share property names must
/// narrow independently: narrowing `first.kind` must not leak into
/// `second.kind`. Binder names are deliberately varied from the usual `x`/`y`.
#[test]
fn distinct_bases_with_shared_property_narrow_independently() {
    let codes = check_source_strict_codes(
        r#"
type Shape = { kind: "a"; av: number } | { kind: "b"; bv: string };
function check(first: Shape, second: Shape) {
  if (first.kind === "a") {
    const n: number = first.av;
  }
  if (second.kind === "b") {
    const s: string = second.bv;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "distinct bases sharing a property name must narrow independently, got: {codes:?}"
    );
}

/// The structural key must include the property: narrowing `box.left` must not
/// be reused for the *different* property `box.right` on the same base, so the
/// still-optional `box.right` keeps `number | undefined` and trips TS2322.
#[test]
fn distinct_properties_on_same_base_do_not_share_narrowing() {
    let codes = check_source_strict_codes(
        r#"
function pick(box: { left?: number; right?: number }) {
  if (box.left !== undefined) {
    const x: number = box.left;
  }
  const y: number = box.right;
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "narrowing box.left must not leak to box.right, expected TS2322, got: {codes:?}"
    );
}

/// Repeated reads of the *same* narrowed path must keep the narrowed type. A
/// shared structural key is read once per occurrence; a corrupted cache entry
/// would surface as `av` no longer existing on the un-narrowed union.
#[test]
fn repeated_reads_of_same_path_keep_narrowing() {
    let codes = check_source_strict_codes(
        r#"
type Shape = { kind: "a"; av: number } | { kind: "b"; bv: string };
function many(value: Shape) {
  if (value.kind === "a") {
    const a1: number = value.av;
    const a2: number = value.av;
    const a3: number = value.av;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "repeated reads of value.av must stay narrowed, got: {codes:?}"
    );
}

/// Deeper paths (`outer.inner.kind`) must also stay distinct per full path:
/// narrowing `left.inner.kind` must not affect `right.inner.kind`.
#[test]
fn distinct_bases_with_shared_nested_path_narrow_independently() {
    let codes = check_source_strict_codes(
        r#"
type Inner = { kind: "a"; av: number } | { kind: "b"; bv: string };
function check(left: { inner: Inner }, right: { inner: Inner }) {
  if (left.inner.kind === "a") {
    const n: number = left.inner.av;
  }
  if (right.inner.kind === "b") {
    const s: string = right.inner.bv;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "nested paths under distinct bases must narrow independently, got: {codes:?}"
    );
}

/// Element-access (`obj["a"]`) and property-access (`obj.a`) denote the same
/// reference, so narrowing through one is observed through the other — the
/// structural key treats them identically.
#[test]
fn element_access_and_property_access_share_one_path() {
    let codes = check_source_strict_codes(
        r#"
function f(obj: { a?: { b: number } }) {
  if (obj["a"]) {
    const v: number = obj.a.b;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "obj[\"a\"] and obj.a are the same reference and must share narrowing, got: {codes:?}"
    );
}
