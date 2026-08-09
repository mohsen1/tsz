//! Regression tests for tsc's `aliasSymbol` retention policy on generic
//! type-alias applications in assignability diagnostics (issue #15368).
//!
//! The structural rule: instantiation that *resolves the type away* — a
//! conditional whose branch is taken, a resolved indexed access or `keyof`,
//! or an alias-forwarding chain bottoming out at one of those — drops the
//! alias symbol, so the diagnostic renders the evaluated type. Constructors
//! that *survive* instantiation (mapped, union, object) keep the alias and
//! render `Name<Args>`; a nullable union target that keeps its alias is not
//! stripped to its non-nullish member.
//!
//! The same alias restoration also governs the *source* side: when the target is
//! a generic union-alias application whose whole (restored) type is singleton-
//! capable through a `null`/`undefined` member, a fresh literal source is kept
//! (`Type '5'`) rather than generalized to its base (`Type 'number'`), mirroring
//! tsc's `reportErrorResults` restoring `originalTarget` before the source-literal
//! generalization gate.
//!
//! Owners: `tsz_solver::diagnostics::format::application_reduction` (shared
//! display reduction), `type_queries::application_base_reducing_alias_body_kind`,
//! and the checker's `render_missing_property` primitive-source target display.

use crate::test_utils::check_source_diagnostics;

/// The single TS2322 message produced by `source`.
fn ts2322_message(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2322[0].message_text.clone()
}

// ── Reducing bodies drop the alias ──

#[test]
fn indexed_access_bodied_alias_application_renders_member_type() {
    let message = ts2322_message(
        r#"
type PickInner<Q extends { inner: unknown }> = Q['inner'];
const wrong: PickInner<{ inner: { deep: boolean } }> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ deep: boolean; }'."
    );
}

#[test]
fn keyof_bodied_alias_application_ranks_keys_like_tsc() {
    let message = ts2322_message(
        r#"
type KeysOf<Rec> = keyof Rec;
const wrong: KeysOf<{ zebra: 1; apple: 2 }> = 5;
"#,
    );
    // tsc 7.0.2 ranks/alphabetizes the resolved key union in display, NOT
    // property declaration order. Oracle (node typescript@7.0.2):
    //   `KeysOf<{ zebra: 1; apple: 2 }> = 5`
    //   → "Type '5' is not assignable to type '"apple" | "zebra"'."
    // (The earlier declaration-order expectation was TS6-era policy the 7.0.2
    // oracle refuted; the keyof reduction now routes through the shared union
    // comparator via `ApplicationDisplayReduction::SortedUnion`.)
    assert_eq!(
        message,
        "Type '5' is not assignable to type '\"apple\" | \"zebra\"'."
    );
}

#[test]
fn object_rest_this_omit_receiver_ranks_string_then_numeric_keys() {
    // The `Omit<this, K>` receiver display for object-rest destructuring ranks
    // the omitted-key union in tsc's union order (oracle: tsc 7.0.2), NOT the
    // property-collection order: string-literal keys quoted + lexicographic
    // first, then number-literal keys UNQUOTED + numeric.
    //   `Omit<this, "alpha" | "beta" | "method" | 2 | 5 | 10>`
    // Guards the numeric-key case: plain lexicographic sort would misorder
    // (`10` before `2`) and mis-quote (`"2"`).
    let diags = check_source_diagnostics(
        r#"
class A {
  10() { return 1; }
  2() { return 2; }
  get 5() { return 3; }
  beta() { return 4; }
  alpha() { return 5; }
  method() {
    const { ...rest } = this;
    rest.alpha;
  }
}
"#,
    );
    let msg = diags
        .iter()
        .find(|d| d.code == 2339 && d.message_text.contains("Omit<this"))
        .map(|d| d.message_text.clone())
        .unwrap_or_default();
    assert_eq!(
        msg,
        "Property 'alpha' does not exist on type 'Omit<this, \"alpha\" | \"beta\" | \"method\" | 2 | 5 | 10>'."
    );
}

#[test]
fn alias_forwarding_to_conditional_alias_renders_resolved_branch() {
    let message = ts2322_message(
        r#"
type Choose<V> = V extends string ? { picked: V } : { fallback: V };
type Forwarded<W> = Choose<W>;
const wrong: Forwarded<string> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ picked: string; }'."
    );
}

#[test]
fn converging_recursive_conditional_alias_renders_reduced_primitive() {
    let message = ts2322_message(
        r#"
type Unwrap<E> = E extends readonly (infer Inner)[] ? Unwrap<Inner> : E;
const wrong: Unwrap<string[][]> = 5;
"#,
    );
    assert_eq!(message, "Type 'number' is not assignable to type 'string'.");
}

#[test]
fn converging_recursive_conditional_alias_renders_reduced_object() {
    let message = ts2322_message(
        r#"
type Peel<E> = E extends readonly (infer Inner)[] ? Peel<Inner> : E;
const wrong: Peel<{ leaf: 1 }[][]> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ leaf: 1; }'."
    );
}

// ── Surviving constructors keep the alias ──

#[test]
fn mapped_bodied_alias_application_keeps_alias_name() {
    let message = ts2322_message(
        r#"
type Identityish<S> = { [K in keyof S]: S[K] };
const wrong: Identityish<{ m: string }> = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type 'Identityish<{ m: string; }>'."
    );
}

#[test]
fn union_bodied_alias_application_keeps_alias_and_is_not_nullish_stripped() {
    let message = ts2322_message(
        r#"
type OrMissing<S> = S | undefined;
const wrong: OrMissing<{ u: string }> = 5;
"#,
    );
    // The alias is restored over the reduced target (tsc `reportErrorResults`),
    // so the reported target is the whole `OrMissing<{ u: string; }>` union —
    // singleton-capable through its `undefined` member — and the literal source
    // `5` is preserved rather than generalized to `number`. Oracle
    // `typescript@7.0.2`: `Type '5' is not assignable to type
    // 'OrMissing<{ u: string; }>'.`
    assert_eq!(
        message,
        "Type '5' is not assignable to type 'OrMissing<{ u: string; }>'."
    );
}

#[test]
fn non_generic_union_alias_target_keeps_alias_name() {
    let message = ts2322_message(
        r#"
type MaybeBox = { u: string } | undefined;
const wrong: MaybeBox = 5;
"#,
    );
    // Residual: the target name `MaybeBox` is kept, but the literal source is
    // still widened. The alias-restore source-literal fix lands for *generic*
    // alias applications (the `OrMissing<..>` case above), which reach the
    // source-display gate as an `Application` carrying the alias surface; a
    // *non-generic* alias reaches it already reduced, so `original_target` no
    // longer answers `type_keeps_alias_symbol_surface`. Oracle `typescript@7.0.2`
    // preserves the literal here too (`Type '5' is not assignable to type
    // 'MaybeBox'.`); pinning current output so a follow-up on the non-generic
    // path updates this deliberately rather than silently.
    assert_eq!(
        message,
        "Type 'number' is not assignable to type 'MaybeBox'."
    );
}

#[test]
fn anonymous_nullable_union_target_still_strips_to_non_nullish_member() {
    let message = ts2322_message(
        r#"
const wrong: { u: string } | undefined = 5;
"#,
    );
    assert_eq!(
        message,
        "Type 'number' is not assignable to type '{ u: string; }'."
    );
}

// ── Negative / fallback cases ──

#[test]
fn still_generic_reducing_application_keeps_alias_spelling() {
    // A free type parameter defers the reduction; tsc keeps `Name<Args>`.
    let diags = check_source_diagnostics(
        r#"
type Choose<V> = V extends string ? { picked: V } : { fallback: V };
function take<W>(w: W) {
    const wrong: string = null as unknown as Choose<W>;
}
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.starts_with("Type 'Choose<W>'"),
        "a still-generic conditional application keeps its alias spelling, got: {}",
        ts2322[0].message_text
    );
}

#[test]
fn non_converging_recursive_tuple_alias_keeps_alias_annotation() {
    // The recursive tuple alias never converges; expanding it would render a
    // truncated cycle, so the annotation surface is preserved.
    let diags = check_source_diagnostics(
        r#"
type Nest<T> = [42, Nest<{ x: T }>];
declare const n: Nest<number>;
const wrong: string = n;
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.contains("Nest<"),
        "a non-converging recursive alias keeps its alias surface, got: {}",
        ts2322[0].message_text
    );
}

#[test]
fn distributive_conditional_over_inline_union_renders_distributed_branches() {
    // Self-contained `Omit` equivalent (the unit harness carries no lib).
    let message = ts2322_message(
        r#"
type Excl<A, B> = A extends B ? never : A;
type DropKey<T, K extends string> = { [P in Excl<keyof T, K>]: T[P] };
type NoC<T> = T extends unknown ? DropKey<T, 'c'> : never;
declare const val: NoC<{ kind: 'a'; x: number; c: boolean } | { kind: 'b'; y: string; c: boolean }>;
const wrong: { kind: 'a'; x: number; c: boolean } = val;
"#,
    );
    assert_eq!(
        message,
        "Type 'DropKey<{ kind: \"a\"; x: number; c: boolean; }, \"c\"> | DropKey<{ kind: \"b\"; y: string; c: boolean; }, \"c\">' is not assignable to type '{ kind: \"a\"; x: number; c: boolean; }'."
    );
}

#[test]
fn keyof_alias_with_numeric_keys_keeps_alias_name() {
    // Numeric property names resolve to number-literal keys, which the
    // declaration-order reconstruction does not cover; the alias surface is
    // preserved rather than rendering a mis-ordered union.
    let diags = check_source_diagnostics(
        r#"
type KeysOf<Rec> = keyof Rec;
const wrong: KeysOf<{ 1: true; 0: false }> = 'nope';
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(ts2322.len(), 1, "diags: {diags:?}");
    assert!(
        ts2322[0].message_text.contains("KeysOf<")
            || ts2322[0].message_text.contains("0 | 1")
            || ts2322[0].message_text.contains("1 | 0"),
        "numeric keyof falls back without asserting a specific order, got: {}",
        ts2322[0].message_text
    );
}

#[test]
fn recursive_non_generic_alias_over_interface_application_keeps_alias_name() {
    // Conformance witness `recursiveTypeReferences1.ts` (`type Box2 =
    // Box<Box2 | number>`): the annotation restores its alias surface, and a
    // recursive non-generic alias renders its *name* — the general formatter
    // would unroll the cycle one evaluation step per render
    // (`Wrap<number | Wrap<number | Cyc>>`), where tsc keeps `Cyc`.
    let message = ts2322_message(
        r#"
interface Wrap_Qx<T> { value: T }
type Cyc_Qx = Wrap_Qx<Cyc_Qx | number>;
const sink_qx: Cyc_Qx = 42;
"#,
    );
    assert_eq!(message, "Type 'number' is not assignable to type 'Cyc_Qx'.");
}

#[test]
fn non_converging_recursive_generic_alias_unroll_keeps_annotation_surface() {
    // The converges gate requires the recursion to have *resolved away*: an
    // evaluation that still mentions the alias cycle (here the object unroll
    // `{ v: RObj_Qx<number> }`) keeps the annotation spelling even though the
    // unrolled shape is concrete and displayable.
    let message = ts2322_message(
        r#"
type RObj_Qx<T> = { v: RObj_Qx<T> } | T;
declare const probe_qx: RObj_Qx<string>;
const sink_qx: 0 = probe_qx;
"#,
    );
    assert!(
        message.contains("RObj_Qx<string>"),
        "self-referential unroll must keep the annotation surface, got: {message}"
    );
}

// ── Property-receiver (TS2339) display follows the same alias-drop rule ──
//
// The `Property 'p' does not exist on type 'X'` receiver renders the same way a
// TS2322 target does: a conditional-type alias whose branch is taken drops its
// own name and shows the resolved branch application, while a plain
// generic/utility alias keeps its name. Varied binder names guard against a
// fixture-scoped display shortcut (issue #14141).

/// The single TS2339 message produced by `source`.
fn ts2339_message(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected exactly one TS2339. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2339[0].message_text.clone()
}

#[test]
fn conditional_alias_receiver_renders_resolved_branch_application() {
    // `Frozen<Widget>` resolves (Widget matches the `{ id: number }` check) to
    // the taken branch `FrozenObject<Widget>`; the conditional alias `Frozen` is
    // dropped, exactly as tsc renders the `conditionalTypes1` `DeepReadonly<Part>`
    // / `DeepReadonlyObject<Part>` receiver.
    let message = ts2339_message(
        r#"
interface Widget { id: number; render: number; }
type Frozen<A> = A extends { id: number } ? FrozenObject<A> : A;
type FrozenObject<A> = { readonly [P in keyof A]: A[P] };

function use_frozen(w: Frozen<Widget>) {
    w.missing;
}
"#,
    );
    assert_eq!(
        message,
        "Property 'missing' does not exist on type 'FrozenObject<Widget>'."
    );
}

#[test]
fn conditional_alias_receiver_resolved_branch_survives_renamed_binders() {
    // Same shape, entirely different binder names: the resolved branch alias is
    // computed structurally, not matched against a fixture identifier.
    let message = ts2339_message(
        r#"
interface Zeta { count: number; tick: number; }
type Sealed<Q> = Q extends { count: number } ? SealedRec<Q> : Q;
type SealedRec<Q> = { readonly [P in keyof Q]: Q[P] };

function use_sealed(z: Sealed<Zeta>) {
    z.absent;
}
"#,
    );
    assert_eq!(
        message,
        "Property 'absent' does not exist on type 'SealedRec<Zeta>'."
    );
}

#[test]
fn non_conditional_generic_alias_receiver_keeps_its_name() {
    // Control: a plain generic mapped-type alias survives instantiation and
    // keeps its own name in the receiver display (it is not a reducing operator),
    // so the alias-drop rule must NOT strip it.
    let message = ts2339_message(
        r#"
interface Widget { id: number; render: number; }
type Frozen<A> = { readonly [P in keyof A]: A[P] };

function use_frozen(w: Frozen<Widget>) {
    w.missing;
}
"#,
    );
    assert_eq!(
        message,
        "Property 'missing' does not exist on type 'Frozen<Widget>'."
    );
}
