//! Display parity for concrete indexed-access types in assignability
//! diagnostics.
//!
//! `tsc` resolves a *concrete* indexed-access type (`Obj["m"]` whose object and
//! key are fully resolved, with no free type parameters) to its member type
//! during type construction (`getIndexedAccessType`), so the type is never an
//! indexed access by the time a diagnostic renders it — `tsc` shows the reduced
//! member shape. tsz kept the access deferred and rendered the unreduced
//! `Obj["m"]` surface in `TS2741`/`TS2322` assignability messages.
//!
//! The display policy now reduces a bare, type-parameter-free indexed access
//! with a literal-shaped key — a single literal, a union of literals, a
//! `keyof` query, or `number` against an array/tuple-shaped object — to its
//! member type for the assignment source/target roles, matching `tsc`.
//! Generic/deferred accesses (a free type parameter in the object or key)
//! stay opaque — `tsc` renders `T["m"]` there too — and are guarded by the
//! existing `deferred_keyof_index_access_assignability_tests` /
//! `deferred_conditional_indexed_access_tests` suites.

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2741_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2741)
        .map(|(_, message)| message)
        .collect()
}

fn assert_reduced_member(message: &str, expected_member: &str) {
    assert!(
        message.contains(expected_member),
        "source must render the reduced member shape `{expected_member}`: {message}"
    );
    assert!(
        !message.contains("[\"") && !message.contains("['"),
        "source must not render the unreduced indexed-access surface: {message}"
    );
}

/// String-literal key: `Obj["m"]` renders as its member object, not `Obj["m"]`.
#[test]
fn concrete_string_indexed_access_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Obj { m: { foo: string } }
declare function get(): Obj["m"];
const bad: { foo: string; bar: string } = get();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ foo: string; }");
}

/// Anti-hardcoding: the reduction keys on the structural concrete-indexed-access
/// condition, not on the identifiers `Obj`/`m`. Renamed binders behave the same.
#[test]
fn concrete_indexed_access_source_reduction_is_binder_name_independent() {
    let messages = ts2741_messages(
        r#"
interface Container { payload: { alpha: string } }
declare function fetchPayload(): Container["payload"];
const bad: { alpha: string; beta: string } = fetchPayload();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ alpha: string; }");
}

/// Numeric-literal key: `Wrap[0]` reduces the same way.
#[test]
fn concrete_numeric_indexed_access_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Wrap { 0: { a: number } }
declare function g(): Wrap[0];
const bad: { a: number; b: number } = g();
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ a: number; }");
}

/// The reduced member must still be related structurally, so a target that the
/// member *does* satisfy stays clean — the reduction is display-only and never
/// fabricates or suppresses a mismatch.
#[test]
fn concrete_indexed_access_assignable_target_stays_clean() {
    let messages = ts2741_messages(
        r#"
interface Obj { m: { foo: string } }
declare function get(): Obj["m"];
const ok: { foo: string } = get();
"#,
    );
    assert!(
        messages.is_empty(),
        "an assignable indexed-access member must not error: {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// Target role.
//
// The source-role reduction above worked because the value's type had already
// been evaluated by the time it reached the formatter. A *target* annotation
// reaches the formatter as the written indexed access, and when its object
// operand is still an unresolved semantic reference — which is what every
// interface and class name is until it is materialized — the reduction had no
// members to index and declined, so the unreduced `Iface["m"]` surface survived
// into the message. Same tsc rule, same display policy; the solver now
// materializes the object operand for the reduction.
// ---------------------------------------------------------------------------

fn strict_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2741 || *code == 2322)
        .map(|(_, message)| message)
        .collect()
}

/// An interface member reached through a target annotation renders the reduced
/// member shape, exactly as the source role already did.
#[test]
fn concrete_indexed_access_target_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Nest { inner: { p: number; q: number } }
const c: Nest["inner"] = { p: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ p: number; q: number; }");
}

/// Anti-hardcoding: the target-role reduction keys on the structural condition
/// (non-generic definition, literal key), never on the binder names.
#[test]
fn concrete_indexed_access_target_reduction_is_binder_name_independent() {
    let messages = ts2741_messages(
        r#"
interface Renamed { payload: { alpha: number; beta: number } }
const b: Renamed["payload"] = { alpha: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ alpha: number; beta: number; }");
}

/// A class field reached through `Class["field"]` reduces the same way — the
/// object operand is a semantic reference for classes as well as interfaces.
#[test]
fn concrete_indexed_access_target_reduces_a_class_member() {
    let messages = ts2741_messages(
        r#"
class Holder { field: { u: string; v: string } = { u: "", v: "" } }
const c: Holder["field"] = { u: "x" };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ u: string; v: string; }");
}

/// Numeric-literal key in target position.
#[test]
fn concrete_numeric_indexed_access_target_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Num { 0: { a: number; b: number } }
const d: Num[0] = { a: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ a: number; b: number; }");
}

/// A union key reduces to the union of the members, which tsc reports as
/// `TS2322` with the reduced constituents rather than `TS2741`.
#[test]
fn concrete_union_key_indexed_access_target_renders_reduced_members() {
    let messages = strict_messages(
        r#"
interface UnionKey { x: { s: 1; t: 2 }; y: { s: 1; t: 2 } }
const e: UnionKey["x" | "y"] = { s: 1 };
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "exactly one assignability error: {messages:?}"
    );
    assert!(
        messages[0].contains("{ s: 1; t: 2; }"),
        "target must render the reduced member shape: {}",
        messages[0]
    );
    assert!(
        !messages[0].contains("[\""),
        "target must not render the unreduced indexed-access surface: {}",
        messages[0]
    );
}

/// An alias that merely renames the object (`type A = Iface`) is transparent:
/// the access still reduces.
#[test]
fn concrete_indexed_access_target_reduces_through_an_alias_chain() {
    let messages = ts2741_messages(
        r#"
interface Chain { deep: { g: boolean; h: boolean } }
type ChainAlias = Chain;
const f: ChainAlias["deep"] = { g: true };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ g: boolean; h: boolean; }");
}

/// An *instantiated* generic object operand reduces too — it arrives as an
/// `Application`, not a `Lazy`, so it never needed the materialization step and
/// must keep working.
#[test]
fn concrete_indexed_access_target_reduces_an_instantiated_generic() {
    let messages = ts2741_messages(
        r#"
interface GenBox<T> { v: { one: T; two: T } }
const g: GenBox<number>["v"] = { one: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ one: number; two: number; }");
}

/// Negative control. A *deferred* access over a free type parameter is opaque
/// in tsc too, so it must keep printing `T["m"]` — the materialization is gated
/// on the definition being non-generic precisely so this row cannot move.
#[test]
fn deferred_generic_indexed_access_target_stays_opaque() {
    let messages = strict_messages(
        r#"
function generic<T extends { m: { i: number; j: number } }>(t: T) {
  const i: T["m"] = { i: 1 };
  return i;
}
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "exactly one assignability error: {messages:?}"
    );
    assert!(
        messages[0].contains("T[\"m\"]"),
        "a deferred generic access must stay opaque: {}",
        messages[0]
    );
}

/// Negative control. The reduction is display-only: a target the source really
/// does satisfy stays clean, and a genuinely missing member still errors.
#[test]
fn concrete_indexed_access_target_assignable_stays_clean() {
    let messages = ts2741_messages(
        r#"
interface Clean { part: { only: number } }
const ok: Clean["part"] = { only: 1 };
"#,
    );
    assert!(
        messages.is_empty(),
        "an assignable indexed-access target must not error: {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// Chained access.
//
// A chain nests one access inside the next, so reducing only the innermost link
// would leave a hybrid: a resolved inner object carrying the remaining written
// keys, which corresponds to nothing in the source and grows with nesting
// depth. The reduction therefore runs to a fixed point over the object operand,
// and when the outer link cannot reduce, the whole chain prints as written.
// ---------------------------------------------------------------------------

/// A three-link chain rooted at an interface reduces all the way.
#[test]
fn chained_indexed_access_target_reduces_to_a_fixed_point() {
    let messages = ts2741_messages(
        r#"
interface A1 { p: { q: { r: { leaf: number; miss: string } } } }
const a1: A1["p"]["q"]["r"] = { leaf: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ leaf: number; miss: string; }");
}

/// A chain that indexes an array member by a numeric literal reduces to the
/// element type, not to `Elem[][0]`.
#[test]
fn chained_indexed_access_target_reduces_an_array_element() {
    let messages = ts2741_messages(
        r#"
interface L { items: { k1: number; k2: string }[] }
const l1: L["items"][0] = { k1: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k1: number; k2: string; }");
}

/// An alias-rooted chain already reduced before this change and must keep
/// doing so — the alias arrives materialized, so it never needed the fixed
/// point.
#[test]
fn alias_rooted_chained_indexed_access_target_still_reduces() {
    let messages = ts2741_messages(
        r#"
type M = { outer: { mid: { z: number; y: string } } };
const m1: M["outer"]["mid"] = { z: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ z: number; y: string; }");
}

/// Non-literal key, still unreduced in the *target* role (`tsc` reduces it —
/// the one piece of #16443's non-literal-key residual this PR leaves open).
/// This is a genuinely different gate than the one this suite otherwise
/// exercises: for a target annotation, `keyof Q` never reaches
/// `resolve_concrete_indexed_access_for_display` at all — the annotation's
/// `TypeId` is already the reduced object shape by the time it gets there
/// (confirmed by direct inspection), so the unreduced `Q[keyof Q]` text comes
/// from a separate declared-annotation-text preference specific to the
/// target/assignment-pair display, not from the key-shape gate this PR
/// widens. The identifier-source twin below
/// (`keyof_indexed_access_identifier_source_renders_reduced_member`) *is*
/// closed by this PR, because a source identifier's declared annotation
/// reaches the widened gate as the written (still-deferred) access. Pinned so
/// the remaining gap is visible rather than silently assumed fixed.
#[test]
fn keyof_rooted_indexed_access_target_prints_as_written() {
    let messages = strict_messages(
        r#"
interface Q { only: { c: number; d: string } }
const q1: Q[keyof Q] = { c: 1 };
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "exactly one assignability error: {messages:?}"
    );
    assert!(
        messages[0].contains("Q[keyof Q]"),
        "an unreduced access must print as written: {}",
        messages[0]
    );
}

/// Numeric index into an array-typed member (#16443's non-literal-key
/// residual, closed) in the target role: `Arr["list"][number]` reduces to
/// the element type. What this also pins is the *no-hybrid* invariant: the
/// inner link (`Arr["list"]`, a literal-string key) must resolve to a fixed
/// point before the outer numeric index can inspect its array shape, so the
/// chain either reduces all the way or (per the deferred-generic negative
/// controls elsewhere in this suite) not at all — never a half-resolved
/// hybrid.
#[test]
fn numeric_index_into_array_member_target_renders_reduced_element() {
    let messages = ts2741_messages(
        r#"
interface Arr { list: { w: number; z: string }[] }
const a2: Arr["list"][number] = { w: 1 };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ w: number; z: string; }");
}

// ---------------------------------------------------------------------------
// Source role reached through an *identifier* whose declaration carries the
// indexed-access annotation.
//
// The source-role rows at the top of this file all reach the access through a
// call return, a property access or an argument — positions where the type is
// evaluated before the diagnostic is built, so the reduced member is what the
// display policy receives. A bare identifier is different: the assignment
// display policy prefers the declaration's annotation *as written* over the
// computed type, which painted the unreduced `Obj["m"]` surface straight back
// over the member the role dispatch had already reduced.
//
// `tsc` resolves a concrete indexed access in `getIndexedAccessType`, during
// type construction, so no diagnostic ever sees the access — the annotation
// surface is not something it can prefer. Both annotation-repaint gates (the
// missing-property one and the TS2322 alias-pair one) now decline for a
// declared type the shared display policy reduces. Deferred accesses keep
// their spelling in both, because the same policy declines for them.
//
// Every expectation below is oracle-pinned against `typescript@7.0.2` with the
// conformance gate's own flags (`--singleThreaded --stableTypeOrdering true`,
// see #16457).
// ---------------------------------------------------------------------------

/// The witness: a `const` annotated with a concrete indexed access renders the
/// reduced member, not the annotation as written.
#[test]
fn concrete_indexed_access_identifier_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Missing { only: { k: number } }
declare const bad: Missing["only"];
const h: { k: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// Anti-hardcoding: the gate keys on the declared type being a reducible
/// concrete access, never on the binder spellings.
#[test]
fn concrete_indexed_access_identifier_source_is_binder_name_independent() {
    let messages = ts2741_messages(
        r#"
interface Wrapper { payload: { alpha: number } }
declare const value: Wrapper["payload"];
const dest: { alpha: number; beta: number } = value;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ alpha: number; }");
}

/// A `return` statement is the same assignment-source role reached through a
/// different anchor.
#[test]
fn concrete_indexed_access_identifier_source_reduces_in_a_return_statement() {
    let messages = ts2741_messages(
        r#"
interface Missing { only: { k: number } }
declare const bad: Missing["only"];
function f(): { k: number; extra: number } { return bad; }
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// A function parameter's annotation is the same declaration shape as a
/// variable's.
#[test]
fn concrete_indexed_access_parameter_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Missing { only: { k: number } }
function g(bad: Missing["only"]) {
  const h: { k: number; extra: number } = bad;
}
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// A class member reached through `Class["field"]` in the annotation.
#[test]
fn concrete_indexed_access_identifier_source_reduces_a_class_member() {
    let messages = ts2741_messages(
        r#"
class Holder { slot!: { k: number }; }
declare const bad: Holder["slot"];
const h: { k: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// Numeric-literal key in the declared annotation.
#[test]
fn concrete_numeric_indexed_access_identifier_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Wrap { 0: { a: number } }
declare const bad: Wrap[0];
const h: { a: number; b: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ a: number; }");
}

/// A three-link chain in the annotation reduces all the way, never to a hybrid
/// of a resolved inner object and the remaining written keys.
#[test]
fn chained_indexed_access_identifier_source_reduces_to_a_fixed_point() {
    let messages = ts2741_messages(
        r#"
interface A1 { p: { q: { leaf: number } } }
declare const bad: A1["p"]["q"];
const h: { leaf: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ leaf: number; }");
}

/// An instantiated generic object operand arrives as an `Application` and
/// reduces the same way.
#[test]
fn concrete_indexed_access_identifier_source_reduces_an_instantiated_generic() {
    let messages = ts2741_messages(
        r#"
interface Box<T> { item: T }
declare const bad: Box<{ k: number }>["item"];
const h: { k: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// An alias that merely renames the object is transparent.
#[test]
fn concrete_indexed_access_identifier_source_reduces_through_an_alias_chain() {
    let messages = ts2741_messages(
        r#"
interface Missing { only: { k: number } }
type Alias = Missing;
declare const bad: Alias["only"];
const h: { k: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// The TS2322 whole-type message takes a *different* annotation-repaint gate
/// (the generic-alias assignment-pair rewrite), so it needs its own row: a
/// property-type mismatch renders the reduced member too.
#[test]
fn concrete_indexed_access_identifier_source_reduces_in_a_ts2322_message() {
    let messages = strict_messages(
        r#"
interface Missing { only: { k: string } }
declare const bad: Missing["only"];
const h: { k: number } = bad;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '{ k: string; }' is not assignable")),
        "TS2322 source must render the reduced member: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("[\"")),
        "no unreduced indexed-access surface may survive: {messages:?}"
    );
}

/// Negative control for the missing-property gate. A *deferred* access over a
/// free type parameter is opaque in tsc too, so the annotation surface stays.
#[test]
fn deferred_generic_indexed_access_identifier_source_stays_opaque() {
    let messages = ts2741_messages(
        r#"
interface Missing { only: { k: number } }
function f<T extends Missing>(bad: T["only"]) {
  const h: { k: number; extra: number } = bad;
}
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert!(
        messages[0].contains("T[\"only\"]"),
        "a deferred access keeps its written spelling: {}",
        messages[0]
    );
}

/// Negative control for the TS2322 gate, same deferred shape.
#[test]
fn deferred_generic_indexed_access_identifier_source_stays_opaque_in_ts2322() {
    let messages = strict_messages(
        r#"
interface Missing { only: { k: string } }
function f<T extends Missing>(bad: T["only"]) {
  const h: { k: number } = bad;
}
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("T[\"only\"]")),
        "a deferred access keeps its written spelling: {messages:?}"
    );
}

/// Negative control. The reduction is display-only: an annotation the target
/// really does accept stays clean.
#[test]
fn concrete_indexed_access_identifier_source_assignable_stays_clean() {
    let messages = strict_messages(
        r#"
interface Missing { only: { k: number } }
declare const bad: Missing["only"];
const h: { k: number } = bad;
"#,
    );
    assert!(messages.is_empty(), "must stay clean: {messages:?}");
}

/// Numeric index into an array-typed member (#16443's non-literal-key
/// residual, closed): `Arr["list"][number]` reduces to the element type
/// instead of printing the written chain. The outer link's key is the
/// `number` intrinsic, not a literal — it only reduces because the inner
/// link (`Arr["list"]`, a literal-string key) is resolved to a fixed point
/// first, exposing the array shape the outer numeric index needs.
#[test]
fn numeric_index_into_array_member_identifier_source_renders_reduced_element() {
    let messages = ts2741_messages(
        r#"
interface Arr { list: { k: number }[] }
declare const bad: Arr["list"][number];
const h: { k: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ k: number; }");
}

/// `keyof` key (#16443's non-literal-key residual, closed): `Q[keyof Q]`
/// reduces to `Q`'s own member type, matching tsc's eager
/// `getIndexedAccessType`.
#[test]
fn keyof_indexed_access_identifier_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface Q { only: { c: number; d: string } }
declare const bad: Q[keyof Q];
const h: { c: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ c: number; d: string; }");
}

/// Literal-union key (#16443's non-literal-key residual, closed) in the
/// identifier-source role — the target-role twin
/// (`concrete_union_key_indexed_access_target_renders_reduced_members`,
/// above) already reduced before this change; the source role reached the
/// same guard through the declared-annotation preference gate and needed the
/// same widening.
#[test]
fn union_key_indexed_access_identifier_source_renders_reduced_member() {
    let messages = ts2741_messages(
        r#"
interface UnionKey { x: { s: number }; y: { s: number } }
declare const bad: UnionKey["x" | "y"];
const h: { s: number; extra: number } = bad;
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2741: {messages:?}");
    assert_reduced_member(&messages[0], "{ s: number; }");
}
