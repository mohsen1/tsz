//! Regression tests for the `TS18032` related-info line `tsc` attaches to a
//! `TS2339` property access on an intersection reduced to `never` because of
//! a private-brand conflict — the sibling of `TS18031`
//! (`ts18031_intersection_conflicting_property_tests.rs`), which handles the
//! *literal-discriminant* reduction reason instead.
//!
//! Structural rule (pinned against `typescript@7.0.2`): when a *directly
//! written* object intersection (`declare const c: A & B`) has two or more
//! members that each declare a property of the same name, and at least one
//! of those declarations is modifier-`private`, `tsc`'s `getReducedType`
//! collapses the intersection to `never` (the private member's nominal brand
//! makes the property un-satisfiable across constituents, regardless of
//! whether the *types* agree) and any property access on it reports
//! `TS2339` with a `relatedInformation` entry: "The intersection '{0}' was
//! reduced to 'never' because property '{1}' exists in multiple constituents
//! and is private in some."
//!
//! Both modifier-`private` on both sides AND modifier-`private` on only one
//! side (paired with a structurally-identical `public` member) trigger this
//! — oracle-verified with an identical-type control, so the conflict is
//! genuinely about the private brand, not a type mismatch a different
//! diagnostic would already catch.
//!
//! Owned by the same `error_reporter/intersection_never_elaboration.rs`
//! helper as TS18031 (`intersection_reduced_to_never_related_info`), which
//! tries the literal-discriminant query first and falls back to
//! `find_private_brand_conflict_property`
//! (`tsz-solver/src/type_queries/data/intersection_conflict.rs`).
//!
//! Deliberately narrow scope, matching TS18031's own precedent: only a
//! directly-written intersection annotation (no alias/generic-application/
//! heritage indirection). Cases that don't reduce to `never` at all
//! (protected-only conflicts report `TS2445` instead; ES `#`-private
//! members don't merge into a brand conflict at all; the same class
//! intersected with itself is not a conflict) are documented as negative
//! controls below — they never reach this helper's `type_id == NEVER` gate,
//! so they're regression coverage for the surrounding machinery, not for the
//! new query itself.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_strict;
use tsz_common::diagnostics::diagnostic_codes;

const TS2339: u32 = diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE;
const TS18032: u32 =
    diagnostic_codes::THE_INTERSECTION_WAS_REDUCED_TO_NEVER_BECAUSE_PROPERTY_EXISTS_IN_MULTIPLE_CONSTI;

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
// Positive cases: a directly-written intersection with a private-brand
// conflict carries the TS18032 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn both_sides_private_carry_ts18032_elaboration() {
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { private x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P1 & P2' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn one_side_private_one_side_public_still_conflicts() {
    // Identical property type on both sides — the conflict is the private
    // brand, not a structural type mismatch a different check would catch.
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'P1 & P2' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn renamed_binders_carry_ts18032_elaboration() {
    // Same rule, different identifiers — structural, not name-keyed.
    let diags = check_source_strict(
        r#"
class Alpha { private val: number = 0; }
class Beta { private val: number = 0; }
declare const combined: Alpha & Beta;
combined.val;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'Alpha & Beta' was reduced to 'never' because property 'val' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn accessing_a_different_property_still_carries_ts18032() {
    let diags = check_source_strict(
        r#"
class WithSecret { private secret: string = ""; }
class AlsoSecret { private secret: string = ""; }
declare const anything: WithSecret & AlsoSecret;
anything.other;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'WithSecret & AlsoSecret' was reduced to 'never' because property 'secret' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / control cases: no TS18032 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn protected_only_conflict_does_not_reduce_to_never() {
    // tsc reports TS2445 (protected access) against the un-reduced
    // intersection type here, not TS2339 against `never` — this shape never
    // reaches the never-elaboration helper at all.
    let diags = check_source_strict(
        r#"
class P1 { protected x: string = ""; }
class P2 { protected x: string = ""; }
declare const c: P1 & P2;
c.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "protected-only conflicts must not reduce to never, got {diags:?}"
    );
}

#[test]
fn same_class_intersected_with_itself_has_no_conflict() {
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
declare const c: P1 & P1;
c.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "a class intersected with itself must not reduce to never, got {diags:?}"
    );
}

#[test]
fn compatible_public_members_report_nothing() {
    let diags = check_source_strict(
        r#"
class Pub1 { x: string = ""; }
class Pub2 { x: string = ""; }
declare const c: Pub1 & Pub2;
c.x;
"#,
    );
    assert!(diags.is_empty(), "got {diags:?}");
}

#[test]
fn indirect_alias_intersection_has_no_ts18032() {
    // Same scope limit as TS18031: the narrow syntactic walk declines once
    // the receiver's own declared type is an alias rather than a
    // directly-written intersection.
    let diags = check_source_strict(
        r#"
class P1 { private x: string = ""; }
class P2 { private x: string = ""; }
type Combined = P1 & P2;
declare const value: Combined;
value.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert!(related(&diag, TS18032).is_none(), "got {diags:?}");
}
