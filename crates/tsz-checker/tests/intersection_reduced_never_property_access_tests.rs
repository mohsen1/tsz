//! Regression coverage for property access on an object intersection that
//! `tsc` reduces to `never` (`getReducedApparentType`).
//!
//! Structural rule (matches TypeScript 6.0.x): an object-type intersection
//! whose members share a non-optional *discriminant* property with mutually
//! disjoint constituents (`{ kind: "a" } & { kind: "b" }`, disjoint unit
//! literals, or a private/public brand collision) is reduced to `never`.
//! Property access on such a receiver reports TS2339 against `never`, e.g.
//! `Property 'kind' does not exist on type 'never'`.
//!
//! The interner already applies this reduction while every member is concrete,
//! so a *directly written* two-object intersection was handled. The gap this
//! covers: when a conflicting member is reached through a generic application
//! (`WithKind<"a"> & WithKind<"b">`) or a type alias, the member stays deferred
//! at intern time and the conflict is invisible until it is evaluated. The
//! lighter property-access receiver evaluator never re-materialized those
//! members, so tsz resolved the discriminant to its `never` *property* type and
//! silently accepted the access — a false negative versus `tsc`.
//!
//! Binder names are varied per case so the rule is structural, not keyed to a
//! particular identifier.

use tsz_checker::test_utils::check_source_strict_codes;

const TS2339: u32 = 2339; // Property does not exist on type

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

// ---------------------------------------------------------------------------
// Positive cases: the intersection reduces to `never`, so property access
// must report TS2339 (matching tsc).
// ---------------------------------------------------------------------------

#[test]
fn generic_application_discriminant_conflict_reduces_to_never() {
    // tsc: `Property 'kind' does not exist on type 'never'.`
    let diags = codes(
        r#"
type WithKind<K extends string> = { kind: K };
type Combined = WithKind<"a"> & WithKind<"b">;
declare const value: Combined;
const read = value.kind;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "generic-application discriminant conflict must reduce to `never`, got {diags:?}"
    );
}

#[test]
fn mixed_concrete_and_generic_member_reduces_to_never() {
    // A concrete `{ tag: "left" }` alongside a generic application still
    // reduces once the application is evaluated. tsc: TS2339.
    let diags = codes(
        r#"
type Tagged<L extends string> = { tag: L };
type Node = { tag: "left" } & Tagged<"right">;
declare const node: Node;
const t = node.tag;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "mixed concrete/generic discriminant conflict must reduce to `never`, got {diags:?}"
    );
}

#[test]
fn nested_alias_intersection_reduces_to_never() {
    // The conflicting discriminant is buried under a nested alias intersection
    // that also carries a non-conflicting `payload`. tsc: TS2339.
    let diags = codes(
        r#"
type WithMode<M extends string> = { mode: M };
type Left = WithMode<"read"> & { payload: number };
type Right = WithMode<"write"> & { payload: string };
type Merged = Left & Right;
declare const merged: Merged;
const m = merged.mode;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "nested-alias discriminant conflict must reduce to `never`, got {diags:?}"
    );
}

#[test]
fn disjoint_numeric_literals_reduce_to_never() {
    let diags = codes(
        r#"
type Holder<N extends number> = { slot: N };
declare const holder: Holder<1> & Holder<2>;
const s = holder.slot;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "disjoint numeric-literal discriminant must reduce to `never`, got {diags:?}"
    );
}

#[test]
fn interface_heritage_discriminant_conflict_reduces_to_never() {
    // The conflict flows in through interface heritage rather than a direct
    // application member. tsc: TS2339.
    let diags = codes(
        r#"
interface Base<K> { tag: K; }
interface First extends Base<"first"> {}
interface Second extends Base<"second"> {}
declare const both: First & Second;
const b = both.tag;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "interface-heritage discriminant conflict must reduce to `never`, got {diags:?}"
    );
}

#[test]
fn arbitrary_property_on_reduced_never_reports_ts2339() {
    // Once reduced to `never`, *any* property access errors — not only the
    // discriminant that caused the reduction.
    let diags = codes(
        r#"
type WithKey<K extends string> = { key: K };
declare const anything: WithKey<"x"> & WithKey<"y">;
anything.whatever;
"#,
    );
    assert!(
        diags.contains(&TS2339),
        "any property on a reduced-`never` receiver must report TS2339, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: the intersection does NOT reduce to `never`, so property
// access must stay clean (matching tsc). Guards against over-reduction.
// ---------------------------------------------------------------------------

#[test]
fn same_literal_members_do_not_reduce() {
    // `WithKind<"a"> & WithKind<"a">` is inhabited; `kind` stays `"a"`.
    let diags = codes(
        r#"
type WithKind<K extends string> = { kind: K };
declare const value: WithKind<"a"> & WithKind<"a">;
const read: "a" = value.kind;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "identical discriminant members must not reduce, got {diags:?}"
    );
}

#[test]
fn subtype_narrowing_member_does_not_reduce() {
    // `number & 1` narrows to `1`, not `never`.
    let diags = codes(
        r#"
type Slot<N> = { value: N };
declare const value: Slot<number> & Slot<1>;
const read: 1 = value.value;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "subtype-narrowing discriminant member must not reduce, got {diags:?}"
    );
}

#[test]
fn optional_discriminants_do_not_reduce() {
    // Two *optional* conflicting occurrences make only the property `never`,
    // not the whole intersection (tsc keeps the receiver an object).
    let diags = codes(
        r#"
type WithKind<K extends string> = { kind?: K };
declare const value: WithKind<"a"> & WithKind<"b">;
const read = value.kind;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "optional discriminant occurrences must not reduce the receiver, got {diags:?}"
    );
}

#[test]
fn branded_primitive_intersection_does_not_reduce() {
    // A branded primitive (`string & { __brand }`) is a valid, inhabited
    // intersection; its brand property must stay accessible.
    let diags = codes(
        r#"
type Usd = string & { readonly __brand: "usd" };
declare const amount: Usd;
const brand: "usd" = amount.__brand;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "branded primitive intersection must not reduce, got {diags:?}"
    );
}

#[test]
fn non_conflicting_application_intersection_does_not_reduce() {
    // A generic-application member intersected with a disjoint-key object has
    // no shared conflicting property — both members stay accessible.
    let diags = codes(
        r#"
type WithKind<K extends string> = { kind: K };
declare const value: WithKind<"a"> & { extra: number };
const k: "a" = value.kind;
const e: number = value.extra;
"#,
    );
    assert!(
        !diags.contains(&TS2339),
        "non-conflicting application intersection must not reduce, got {diags:?}"
    );
}
