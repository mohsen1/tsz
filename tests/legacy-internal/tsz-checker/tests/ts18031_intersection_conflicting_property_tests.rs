//! Regression tests for the `TS18031` related-info line `tsc` attaches to a
//! `TS2339` property access on an intersection reduced to `never`.
//!
//! Structural rule (pinned against `typescript@7.0.2`): when a *directly
//! written* object intersection (`declare const c: A & B`) has two members
//! whose required literal-typed occurrences of the same property name are
//! mutually exclusive (`{ x: 1 }` vs `{ x: 2 }`), `tsc`'s `getReducedType`
//! collapses the intersection to `never` and any property access on it
//! reports `TS2339` with a `relatedInformation` entry: "The intersection
//! '{0}' was reduced to 'never' because property '{1}' has conflicting types
//! in some constituents." The entry carries no location of its own (a
//! message-chain link, not a cross-location pointer), so it prints in both
//! `--pretty` and plain output alike, unindented in front of a file/line
//! header.
//!
//! `TypeInterner::intern` already collapses such an intersection to the
//! single canonical `TypeId::NEVER` at construction time, so tsz recovers the
//! pre-reduction member list from the receiver's own declared-type syntax
//! (`declared_never_intersection_for_expression`,
//! `error_reporter/core/declared_intersection_display.rs`) rather than from
//! the reported `never` itself, and re-runs a conflict search
//! (`find_disjoint_literal_property_across_intersection`,
//! `tsz-solver/src/type_queries/data/content_predicates.rs`) over the
//! resolved members. Owned by `error_reporter/properties.rs`'s
//! `intersection_reduced_to_never_related_info`.
//!
//! The recovery follows type-alias references (`type C = A & B; declare const
//! c: C`, and multi-hop `type D = C`), naming the alias whose body is directly
//! the intersection exactly as `tsc`'s `typeToString` does. Remaining scope
//! limits (each an under-cover that keeps `TS2339` with no elaboration, never
//! a wrong one): the single-required-literal-per-member discriminant shape
//! only; a generic *alias* whose conflicting property is the unsubstituted
//! type parameter (the reduction to `never` itself does not yet fire there);
//! and a cross-arena alias whose declaration lives in another file.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;
use tsz_common::diagnostics::diagnostic_codes;

const TS2339: u32 = diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE;
const TS18031: u32 =
    diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_HAS_CONFLICTING_TYPES_IN;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

fn related(diagnostic: &Diagnostic, code: u32) -> Option<String> {
    diagnostic
        .related_information
        .iter()
        .find(|info| info.code == code)
        .map(|info| info.message_text.clone())
}

// ---------------------------------------------------------------------------
// Positive cases: a directly-written intersection with a conflicting literal
// discriminant carries the TS18031 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn interface_members_carry_ts18031_elaboration() {
    let diags = check_source_strict(
        r#"
interface Alpha { tag: 1 }
interface Beta { tag: 2 }
declare const value: Alpha & Beta;
value.tag;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Alpha & Beta' was reduced to 'never' because property 'tag' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn type_literal_members_carry_ts18031_elaboration() {
    // Same rule with inline type-literal members instead of named
    // interfaces — the recovered member types are named by their structural
    // display, not by a symbol name.
    let diags = check_source_strict(
        r#"
declare const value: { mode: "left" } & { mode: "right" };
value.mode;
"#,
    );
    let diag = only(&diags, TS2339);
    let msg = related(&diag, TS18031);
    assert!(
        msg.as_deref().is_some_and(|m| m.contains("property 'mode'")
            && m.contains("conflicting types in some constituents")),
        "got {diags:?}"
    );
}

#[test]
fn class_members_carry_ts18031_elaboration() {
    // Binder names varied from the interface case above so the rule is
    // structural, not keyed to a particular identifier.
    let diags = check_source_strict(
        r#"
class First { slot: "a" = "a"; }
class Second { slot: "b" = "b"; }
declare const combined: First & Second;
combined.slot;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'First & Second' was reduced to 'never' because property 'slot' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn numeric_literal_conflict_carries_ts18031_elaboration() {
    let diags = check_source_strict(
        r#"
interface Holder1 { count: 1 }
interface Holder2 { count: 2 }
declare const both: Holder1 & Holder2;
both.count;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Holder1 & Holder2' was reduced to 'never' because property 'count' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn accessing_a_different_property_still_carries_ts18031() {
    // Once reduced to `never`, *any* property access errors — the
    // elaboration still names the discriminant that caused the reduction,
    // not the property actually accessed.
    let diags = check_source_strict(
        r#"
interface WithKeyA { key: "a" }
interface WithKeyB { key: "b" }
declare const anything: WithKeyA & WithKeyB;
anything.whatever;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'WithKeyA & WithKeyB' was reduced to 'never' because property 'key' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / control cases: no TS18031 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn non_intersection_never_receiver_has_no_ts18031() {
    // `never` from ordinary exhaustive narrowing, not from an intersection
    // reduction, must not carry the intersection elaboration.
    let diags = check_source_strict(
        r#"
declare const value: string | number;
if (typeof value === "string") {
} else if (typeof value === "number") {
} else {
    value.toString();
}
"#,
    );
    let diag = only(&diags, TS2339);
    assert!(related(&diag, TS18031).is_none(), "got {diags:?}");
}

#[test]
fn same_literal_members_do_not_reduce_and_report_nothing() {
    let diags = check_source_strict(
        r#"
interface WithKind { kind: "a" }
declare const value: WithKind & WithKind;
const read: "a" = value.kind;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "identical discriminant members must not reduce, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Multi-conflict ordering: when more than one property name conflicts, tsc
// always names the first one by a combined declaration order (walk members
// left to right, and within each member walk its own properties in
// declaration order — a name is positioned at its *first* occurrence).
// Oracle-verified against `typescript@7.0.2` for every case below.
// ---------------------------------------------------------------------------

#[test]
fn multi_conflict_names_first_declared_property_in_first_member() {
    // `x` is declared before `y` in the first member, so `x` wins even
    // though the access is on `x` itself and both `x` and `y` conflict.
    let diags = check_source_strict(
        r#"
interface A { x: 1; y: "a" }
interface B { x: 2; y: "b" }
declare const c: A & B;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'A & B' was reduced to 'never' because property 'x' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_order_follows_first_members_declaration_order_not_alphabetical() {
    // Source order is `zz` then `aa` (reverse of alphabetical); the winner
    // is `zz`, proving the pick is declaration order, not name order.
    let diags = check_source_strict(
        r#"
interface P { zz: 1; aa: "a" }
interface Q { zz: 2; aa: "b" }
declare const r: P & Q;
r.zz;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'P & Q' was reduced to 'never' because property 'zz' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_order_ignores_second_members_declaration_order() {
    // `A` declares `y` before `x`; `B` declares `x` before `y`. tsc uses the
    // FIRST member's order, so `y` wins even though `B` (and the access
    // itself) would suggest `x`.
    let diags = check_source_strict(
        r#"
interface A { y: "a"; x: 1 }
interface B { x: 2; y: "b" }
declare const c: A & B;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'A & B' was reduced to 'never' because property 'y' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_conflict_order_appends_property_absent_from_first_member() {
    // `A` declares only `p` (no conflict on its own). `B` declares `r` then
    // `q`; `C` declares `q` then `r`. Since `p` never conflicts, the winner
    // is the first NEW name introduced by a later member: `B`'s own order
    // puts `r` before `q`, so `r` wins over `q` even though `C` declares
    // `q` first.
    let diags = check_source_strict(
        r#"
interface A { p: 1 }
interface B { r: "b"; q: "x" }
interface C { q: "y"; r: "c" }
declare const c: A & B & C;
c.p;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'A & B & C' was reduced to 'never' because property 'r' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn alias_to_intersection_carries_ts18031_naming_the_alias() {
    // The receiver's declared type is a *type alias* to the conflicting
    // intersection. The syntactic walk now follows the alias reference to the
    // intersection behind it, and — matching `tsc`'s `typeToString` of the
    // reduced type — names the alias (`Combined`) in the `{0}` slot rather
    // than expanding its members.
    let diags = check_source_strict(
        r#"
interface Left { tag: 1 }
interface Right { tag: 2 }
type Combined = Left & Right;
declare const value: Combined;
value.tag;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Combined' was reduced to 'never' because property 'tag' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn multi_hop_alias_names_the_alias_that_wraps_the_intersection() {
    // `Outer -> Inner -> A & B`: `tsc` names the *innermost* alias whose body
    // is directly the intersection (`Inner`), not the outer alias that merely
    // forwards to it, nor the members.
    let diags = check_source_strict(
        r#"
interface A { tag: 1 }
interface B { tag: 2 }
type Inner = A & B;
type Outer = Inner;
declare const value: Outer;
value.tag;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Inner' was reduced to 'never' because property 'tag' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn alias_of_generic_application_members_materializes_then_conflicts() {
    // The alias body intersects two *generic applications* (`WithKind<"a">`,
    // `WithKind<"b">`). Each must materialize before its literal `kind`
    // property is comparable; once it does, the conflict is found and the
    // alias `Combined` is named. (Witness from issue #15396's `Combined` case.)
    let diags = check_source_strict(
        r#"
type WithKind<K> = { kind: K };
type Combined = WithKind<"a"> & WithKind<"b">;
declare const value: Combined;
value.kind;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Combined' was reduced to 'never' because property 'kind' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn parenthesized_alias_body_still_carries_ts18031() {
    let diags = check_source_strict(
        r#"
interface A { tag: 1 }
interface B { tag: 2 }
type Combined = (A & B);
declare const value: Combined;
value.tag;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Combined' was reduced to 'never' because property 'tag' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}

#[test]
fn alias_intersection_elaboration_is_binder_name_agnostic() {
    // Renaming every binder must not change whether the elaboration fires —
    // only the rendered alias/property names, which track the source verbatim.
    let diags = check_source_strict(
        r#"
interface Zebra { kind: 'x' }
interface Yak { kind: 'y' }
type Quokka = Zebra & Yak;
declare const w: Quokka;
w.kind;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18031).as_deref(),
        Some(
            "The intersection 'Quokka' was reduced to 'never' because property 'kind' has conflicting types in some constituents."
        ),
        "got {diags:?}"
    );
}
