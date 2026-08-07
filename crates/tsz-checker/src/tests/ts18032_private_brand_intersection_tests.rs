//! Regression tests for the `TS18032` related-info line `tsc` attaches to a
//! `TS2339` property access on an intersection reduced to `never` by a
//! private-brand conflict — the sibling of `TS18031`
//! (`ts18031_intersection_conflicting_property_tests.rs`) for the shape that
//! file's own doc comment flags as a separate, then-unimplemented follow-up.
//!
//! Structural rule (pinned against `typescript@7.0.2`): when a *directly
//! written* object intersection (`declare const c: A & B`) has two or more
//! constituents that each declare a property of the same name, and that name
//! is `private` in at least one of them, `tsc` collapses the intersection to
//! `never` and any property access on it reports `TS2339` with a
//! `relatedInformation` entry: "The intersection '{0}' was reduced to
//! 'never' because property '{1}' exists in multiple constituents and is
//! private in some." This fires regardless of whether the conflicting
//! occurrences' types agree — unlike `TS18031`, this is a nominal-identity
//! conflict, not a literal-value conflict.
//!
//! The conflict is keyed on *declaring symbol* identity, not proximity: a
//! property inherited unchanged from a common base class through two
//! subclasses is one occurrence (the base's), not two, so it does not
//! trigger this rule — matching `PropertyInfo::parent_id`, which class-shape
//! construction only rewrites for own/overriding members
//! (`find_private_brand_conflicting_property_across_intersection`,
//! `tsz-solver/src/type_queries/data/intersection_conflict.rs`). Owned by
//! `error_reporter/intersection_never_elaboration.rs`'s
//! `intersection_reduced_to_never_related_info`, as a fallback when the
//! `TS18031` literal-conflict check finds nothing.

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
fn both_private_same_name_carries_ts18032_elaboration() {
    let diags = check_source_strict(
        r#"
class Alpha { private x: number = 1; }
class Beta { private x: string = "a"; }
declare const value: Alpha & Beta;
value.x;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'Alpha & Beta' was reduced to 'never' because property 'x' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn private_and_public_same_name_carries_ts18032_elaboration() {
    // The rule fires as soon as one occurrence is private, regardless of the
    // other's visibility or whether the two types actually agree.
    let diags = check_source_strict(
        r#"
class First { private slot: number = 1; }
class Second { slot: number = 2; }
declare const combined: First & Second;
combined.slot;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'First & Second' was reduced to 'never' because property 'slot' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn private_and_protected_same_name_carries_ts18032_elaboration() {
    let diags = check_source_strict(
        r#"
class Holder1 { private count: number = 1; }
class Holder2 { protected count: number = 2; }
declare const both: Holder1 & Holder2;
both.count;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'Holder1 & Holder2' was reduced to 'never' because property 'count' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

#[test]
fn three_way_intersection_carries_ts18032_elaboration() {
    let diags = check_source_strict(
        r#"
class WithKeyA { private key: number = 1; }
class WithKeyB { private key: number = 2; }
class Plain { other: number = 3; }
declare const anything: WithKeyA & WithKeyB & Plain;
anything.other;
"#,
    );
    let diag = only(&diags, TS2339);
    assert_eq!(
        related(&diag, TS18032).as_deref(),
        Some(
            "The intersection 'WithKeyA & WithKeyB & Plain' was reduced to 'never' because property 'key' exists in multiple constituents and is private in some."
        ),
        "got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / control cases: no TS18032 elaboration.
// ---------------------------------------------------------------------------

#[test]
fn both_protected_same_name_has_no_ts18032() {
    // Protected-only conflicts do not collapse the intersection to `never`;
    // `tsc` reports the ordinary protected-access diagnostic (TS2445) on the
    // still-valid `A & B` type instead.
    let diags = check_source_strict(
        r#"
class Alpha { protected x: number = 1; }
class Beta { protected x: number = 2; }
declare const value: Alpha & Beta;
value.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "protected-only conflicts must not reduce to never, got {diags:?}"
    );
}

#[test]
fn shared_base_private_member_has_no_ts18032() {
    // Both subclasses inherit the exact same private declaration from
    // `Base`, unchanged — one declaring symbol, not two, so this is not a
    // conflict. `tsc` reports the ordinary private-access diagnostic
    // (TS2341) naming the declaring class.
    let diags = check_source_strict(
        r#"
class Base { private x: number = 1; }
class Alpha extends Base {}
class Beta extends Base {}
declare const value: Alpha & Beta;
value.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "a private member inherited unchanged through a shared base must not reduce to never, got {diags:?}"
    );
}

#[test]
fn same_class_twice_has_no_ts18032() {
    let diags = check_source_strict(
        r#"
class Alpha { private x: number = 1; }
declare const value: Alpha & Alpha;
value.x;
"#,
    );
    assert!(
        diags.iter().all(|d| d.code != TS2339),
        "the same class repeated in an intersection must not reduce to never, got {diags:?}"
    );
}

#[test]
fn non_intersection_never_receiver_has_no_ts18032() {
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
    assert!(related(&diag, TS18032).is_none(), "got {diags:?}");
}
