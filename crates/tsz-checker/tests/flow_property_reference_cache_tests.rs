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

// ---------------------------------------------------------------------------
// `this` / `super` bases and non-narrowable member accesses.
//
// `this` and `super` carry no binder symbol, so before they were given a
// reserved structural base component their flow narrowing fell back to a
// per-node cache key and re-walked the flow graph per read (O(n^2) for
// `this`-heavy method bodies). Non-reference member accesses (call results,
// fresh object literals, dynamic element indices) are not references at all:
// tsc never narrows them, so the flow walk is skipped entirely. These tests
// pin both the narrowing that must be preserved and the narrowing that must
// not appear.
// ---------------------------------------------------------------------------

/// `this.foo` narrows like any other reference, and a `this` base must not
/// alias a same-named property on an unrelated receiver.
#[test]
fn this_member_narrows_and_is_receiver_disjoint() {
    let codes = check_source_strict_codes(
        r#"
type Shape = { kind: "a"; av: number } | { kind: "b"; bv: string };
class Holder {
  shape: Shape = { kind: "a", av: 1 };
  inspect(other: Shape) {
    if (this.shape.kind === "a") {
      const n: number = this.shape.av;
    }
    if (other.kind === "b") {
      const s: string = other.bv;
    }
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "this.shape and a same-named param property must narrow independently, got: {codes:?}"
    );
}

/// Repeated reads of `this.value` must keep the narrowed type — the O(n^2) fix
/// must not corrupt the shared narrowing across occurrences.
#[test]
fn repeated_this_reads_keep_narrowing() {
    let codes = check_source_strict_codes(
        r#"
class Box {
  value: string | number = 0;
  use() {
    if (typeof this.value === "string") {
      const a: string = this.value;
      const b: string = this.value;
      const c: string = this.value;
    }
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "repeated this.value reads must stay narrowed to string, got: {codes:?}"
    );
}

/// Assigning to `this.field` narrows subsequent reads (assignment flow).
#[test]
fn this_member_assignment_narrows() {
    let codes = check_source_strict_codes(
        r#"
class Box {
  field: string | number = 0;
  set() {
    this.field = "hi";
    const s: string = this.field;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "this.field must narrow to string after assignment, got: {codes:?}"
    );
}

/// Call results are not references: tsc does not narrow `f().v` across the
/// guard (each call is a fresh value), so the inner read keeps `number |
/// undefined` and trips TS2322. Skipping the flow walk must preserve this.
#[test]
fn call_result_member_is_not_narrowed() {
    let codes = check_source_strict_codes(
        r#"
declare function make(): { v: number | undefined };
function use() {
  if (make().v !== undefined) {
    const x: number = make().v;
  }
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "call-result member access must not narrow (tsc parity), expected TS2322, got: {codes:?}"
    );
}

/// A fresh object literal receiver is not a reference and must not narrow.
#[test]
fn fresh_object_literal_member_is_not_narrowed() {
    let codes = check_source_strict_codes(
        r#"
function use(seed: number | undefined) {
  if (({ v: seed }).v !== undefined) {
    const x: number = ({ v: seed }).v;
  }
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "fresh-object-literal member access must not narrow, expected TS2322, got: {codes:?}"
    );
}

/// `tsc`'s `isNarrowableReference` allows element-access indices that are
/// string/number literals OR entity-name expressions, but not computed
/// expressions. Mirror all three: `arr[i % 3]` (computed) does not narrow and
/// trips TS2322, while `arr[0]` (literal) and `arr[i]` (entity-name index) do
/// narrow. The entity-name case guards against over-skipping: `obj[key]` is
/// narrowable even though it has no stable structural cache key.
#[test]
fn element_index_narrowability_matches_tsc() {
    let computed = check_source_strict_codes(
        r#"
declare const arr: (number | undefined)[];
declare const i: number;
function use() {
  if (arr[i % 3] !== undefined) {
    const x: number = arr[i % 3];
  }
}
"#,
    );
    assert!(
        computed.contains(&2322),
        "arr[i % 3] with a computed index must not narrow, expected TS2322, got: {computed:?}"
    );

    let literal = check_source_strict_codes(
        r#"
declare const arr: (number | undefined)[];
function use() {
  if (arr[0] !== undefined) {
    const x: number = arr[0];
  }
}
"#,
    );
    assert!(
        literal.is_empty(),
        "arr[0] with a literal index must narrow, got: {literal:?}"
    );

    let entity = check_source_strict_codes(
        r#"
declare const arr: (number | undefined)[];
declare const i: number;
function use() {
  if (arr[i] !== undefined) {
    const x: number = arr[i];
  }
}
"#,
    );
    assert!(
        entity.is_empty(),
        "arr[i] with an entity-name index must narrow (it is narrowable in tsc), got: {entity:?}"
    );
}

/// Optional-chain references (`o?.inner?.kind`) are narrowable and their cache
/// key folds the `?.` flag, so repeated reads of the same optional path share
/// (O(n)) while staying correctly narrowed. The structural key must keep the
/// optionality so a mixed `o.a` / `o?.a` program never cross-contaminates.
#[test]
fn optional_chain_member_narrows_and_repeats() {
    let codes = check_source_strict_codes(
        r#"
type Shape = { kind: "a"; av: number } | { kind: "b"; bv: string };
function inspect(o: { inner?: Shape }) {
  if (o.inner?.kind === "a") {
    const a1: number = o.inner.av;
    const a2: number = o.inner.av;
    const a3: number = o.inner.av;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "optional-chain narrowing of o.inner must survive repeated reads, got: {codes:?}"
    );
}

/// Member accesses over the `import.meta` meta-property root are narrowable:
/// `is_matching_reference` treats `import.meta` as a stable reference, so the
/// skip predicate must not classify `import.meta.x` as non-narrowable.
#[test]
fn import_meta_member_is_narrowable() {
    let codes = check_source_strict_codes(
        r#"
interface ImportMeta { value?: { n: number } }
export function read() {
  if (import.meta.value) {
    const n: number = import.meta.value.n;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "import.meta.value member access must narrow, got: {codes:?}"
    );
}
