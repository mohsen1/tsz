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
//! (`declared_intersection_member_types_for_expression`,
//! `error_reporter/core/declared_intersection_display.rs`) rather than from
//! the reported `never` itself, and re-runs a conflict search
//! (`find_disjoint_literal_property_across_intersection`,
//! `tsz-solver/src/type_queries/data/content_predicates.rs`) over the
//! resolved members. Owned by `error_reporter/properties.rs`'s
//! `intersection_reduced_to_never_related_info`.
//!
//! Deliberately narrow scope, matching the helper's own doc comments: only
//! the single-required-literal-per-member discriminant shape, only a
//! directly-written intersection annotation (no alias/generic-application/
//! heritage indirection). Private-brand conflicts are `TS18032`, a separate
//! diagnostic covered by `ts18032_private_brand_intersection_tests.rs`.
//! Every case outside that scope keeps today's behavior (`TS2339` with no
//! elaboration), never a wrong one.

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

#[test]
fn indirect_alias_intersection_has_no_ts18031() {
    // The conflict is real (tsc still reports the elaboration here through
    // its full alias-resolution machinery), but the narrow syntactic walk
    // this diagnostic uses declines once the receiver's own declared type is
    // an alias rather than a directly-written intersection — a documented
    // scope limit, not a false report.
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
    assert!(related(&diag, TS18031).is_none(), "got {diags:?}");
}
