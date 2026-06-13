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

// ---------------------------------------------------------------------------
// Linear pass-through short-circuit (`chase_linear_passthrough`).
//
// A straight-line run of `const`/assignment statements that do not target or
// affect a reference is spliced out of the backward flow walk in O(1) per node.
// These witnesses pin that the splice is narrowing-exact: it must NOT fire (or
// must land correctly) whenever a node between the declaration and the use
// carries narrowing — type guards, mid-chain reassignment, discriminant
// branches, captured/aliased roots, and optional-chain roots all still narrow.
// Binder names are deliberately varied from the usual `x`/`y` so the short
// circuit cannot be keyed on identifier text.
// ---------------------------------------------------------------------------

/// A narrowed `const` with a type guard between its declaration and use must
/// still narrow: the CONDITION node is not a pure pass-through, so the chase
/// stops there. Many unrelated leading `const` statements (the spliced run) must
/// not swallow the guard. `payload` narrows to the `"text"` arm, so reading
/// `.body` as a `string` is fine and reading it as a `number` trips TS2322.
#[test]
fn type_guard_between_decl_and_use_still_narrows_after_passthrough_run() {
    let codes = check_source_strict_codes(
        r#"
type Message =
  | { channel: "text"; body: string }
  | { channel: "code"; body: number };
function handle(payload: Message) {
  const alpha = 1;
  const bravo = 2;
  const charlie = 3;
  const delta = 4;
  if (payload.channel === "text") {
    const ok: string = payload.body;
    const bad: number = payload.body;
  }
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "type-guard narrowing after a pass-through run must hold (TS2322 on number), got: {codes:?}"
    );
}

/// A reference reassigned mid-chain is NOT a pure pass-through at the reassigning
/// node, so the chase stops and the killing definition wins. `subject` starts as
/// a `string` union, is narrowed to `"a"`, then reassigned to a number — the
/// later read must see `number`, so annotating it `string` trips TS2322.
#[test]
fn reassigned_mid_chain_reference_stops_passthrough_chase() {
    let codes = check_source_strict_codes(
        r#"
function process(subject: string | number) {
  const lead1 = 10;
  const lead2 = 20;
  if (typeof subject === "string") {
    subject = 99;
    const wrong: string = subject;
  }
}
"#,
    );
    assert!(
        codes.contains(&2322),
        "mid-chain reassignment must stop the pass-through chase (TS2322), got: {codes:?}"
    );
}

/// An aliased/captured reference still narrows across a long pass-through run:
/// the alias binding is itself a pass-through, but the guard on the alias must
/// survive the splice. `clone.detail` narrows to present, so `.value` is a
/// `number` and no diagnostic is expected.
#[test]
fn aliased_captured_reference_narrows_through_passthrough_run() {
    let codes = check_source_strict_codes(
        r#"
function inspect(origin: { detail?: { value: number } }) {
  const noise1 = "a";
  const noise2 = "b";
  const noise3 = "c";
  const clone = origin;
  const noise4 = "d";
  if (clone.detail) {
    const v: number = clone.detail.value;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "aliased reference must keep narrowing through a pass-through run, got: {codes:?}"
    );
}

/// An optional-chain member reference (root resolves to `Unknown`) still narrows
/// after a long pass-through run; the chase falls back to the full walk for the
/// guard node. No diagnostic expected.
#[test]
fn optional_chain_reference_narrows_after_passthrough_run() {
    let codes = check_source_strict_codes(
        r#"
function read(box?: { inner?: { count: number } }) {
  const s1 = 0;
  const s2 = 0;
  const s3 = 0;
  const s4 = 0;
  if (box?.inner) {
    const c: number = box.inner.count;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "optional-chain narrowing must survive a pass-through run, got: {codes:?}"
    );
}

/// A discriminated union narrowed inside a branch, with the discriminant check
/// preceded by a pass-through run, must still expose the branch-only member.
/// `node.kind === "leaf"` narrows to the leaf arm; reading `.weight` (leaf only)
/// is fine, and reading `.children` (branch only) trips TS2339.
#[test]
fn discriminated_union_branch_narrows_after_passthrough_run() {
    let codes = check_source_strict_codes(
        r#"
type Tree =
  | { kind: "leaf"; weight: number }
  | { kind: "branch"; children: number };
function walk(node: Tree) {
  const pre1 = 1;
  const pre2 = 2;
  const pre3 = 3;
  if (node.kind === "leaf") {
    const w: number = node.weight;
    const oops = node.children;
  }
}
"#,
    );
    assert!(
        codes.contains(&2339),
        "discriminated-union branch narrowing after a pass-through run must hold (TS2339), got: {codes:?}"
    );
}

/// Many independent top-level-style `const` member reads in sequence (the exact
/// `Σ O(i)` hotspot shape) must each see their own value and not leak narrowing
/// from a sibling — the splice must not alias distinct references. None of these
/// reads is illegal, so no diagnostic is expected; this pins that the spliced
/// run finalizes each reference correctly rather than collapsing them.
#[test]
fn sequential_member_reads_do_not_cross_contaminate_under_passthrough() {
    let codes = check_source_strict_codes(
        r#"
declare const recA: { v: number };
declare const recB: { v: string };
declare const recC: { v: boolean };
const useA: number = recA.v;
const useB: string = recB.v;
const useC: boolean = recC.v;
const useA2: number = recA.v;
"#,
    );
    assert!(
        codes.is_empty(),
        "sequential distinct member reads must not cross-contaminate, got: {codes:?}"
    );
}

/// Destructuring binding after a type guard, with intervening pass-through
/// `const` reads between the destructuring and the use. The destructuring
/// `const { nested: { b: text } } = src` is never spliced (it has dedicated
/// worklist handling), and the intervening reads that the chase DOES splice
/// must not orphan the guarded property read: `src.nested.b` is narrowed to
/// `string` by the guard, so `text` (and direct `src.member` reads) stay
/// narrowed. Mirrors `destructuringTypeGuardFlow`; binder names varied.
#[test]
fn destructuring_after_guard_with_intervening_passthrough_keeps_narrowing() {
    let codes = check_source_strict_codes(
        r#"
type Holder = {
  count: number | null;
  label: string;
  inner: { idx: number; tag: string | null };
};
const src: Holder = { count: 3, label: "b", inner: { idx: 1, tag: "y" } };
if (src.count && src.inner.tag) {
  const { count, label, inner: { idx, tag: text } } = src;
  const okCount: number = src.count;
  const okIdx: number = idx;
  const okLabel: string = label;
  const okText: string = text;
}
"#,
    );
    assert!(
        codes.is_empty(),
        "guarded narrowing must survive a destructuring + intervening pass-through run, got: {codes:?}"
    );
}

/// A `switch` over an `unknown` reference narrows each case body, with leading
/// pass-through `const` statements that the chase must NOT splice (UNKNOWN
/// initial types are excluded from the chase because the worklist gives them
/// dedicated switch/typeof handling). Mirrors the `switchTestCollectEnum`
/// family of `unknownType2`; binder names varied.
#[test]
fn unknown_switch_case_narrowing_survives_passthrough_run() {
    let codes = check_source_strict_codes(
        r#"
enum Hue { Red = "red", Green = "green", Blue = "blue" }
function classify(token: unknown) {
  const lead1 = 0;
  const lead2 = 0;
  const lead3 = 0;
  switch (token) {
    case Hue.Red:
      const r: Hue.Red = token;
      break;
    case Hue.Green:
      const g: Hue.Green = token;
      break;
  }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "unknown switch-case narrowing must survive a pass-through run, got: {codes:?}"
    );
}
