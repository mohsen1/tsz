//! Regression tests for redundant-intersection identity in a conditional
//! `extends` clause (issue #16090 / PR #16092).
//!
//! `A & M` where `A` is a subtype of `M` is mutually assignable with `A`
//! alone, but tsc's `isTypeIdenticalTo` does not conflate a redundant
//! intersection with one of its own members. This is the same higher-order
//! `(<T>() => T extends X ? 1 : 2)` identity mechanism covered for
//! readonly/optional property modifiers in
//! `conditional_extends_readonly_identity_tests.rs`; this file pins the
//! intersection-member-subsumption case, including the tuple witness
//! verified against tsc 7.0.2 in the PR.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::{Diagnostic, DiagnosticCategory};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ESNext,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
}

fn error_codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error)
        .map(|d| d.code)
        .collect()
}

const IF_EQUALS_PRELUDE: &str = r#"
type IfEquals<X, Y, A = X, B = never> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? A : B;
"#;

#[test]
fn redundant_tuple_intersection_is_not_identical_to_its_subsumed_member() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<\n\
          readonly [\"alpha\"] & readonly [\"alpha\", ...(string | number | symbol)[]],\n\
          readonly [\"alpha\"],\n\
          \"EQ\",\n\
          \"DIFF\"\n\
        >;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat a redundant tuple intersection as DIFF from its subsumed member; got: {diags:#?}"
    );
}

#[test]
fn redundant_tuple_intersection_under_alpha_renamed_type_parameters() {
    let source = r#"
type Equal<L, R, T = "EQ", F = "DIFF"> =
  (<U>() => U extends L ? 1 : 2) extends
  (<U>() => U extends R ? 1 : 2) ? T : F;
type R = Equal<
  readonly ["first"] & readonly ["first", ...(string | number | symbol)[]],
  readonly ["first"]
>;
const r: R = "DIFF";
"#;
    let diags = check(source);
    assert!(
        error_codes(&diags).is_empty(),
        "Renamed IfEquals should still treat a redundant tuple intersection as DIFF; got: {diags:#?}"
    );
}

#[test]
fn identical_tuples_stay_identical() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<readonly [\"alpha\"], readonly [\"alpha\"], \"EQ\", \"DIFF\">;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat two identical tuples as EQ; got: {diags:#?}"
    );
}

#[test]
fn intersection_member_order_does_not_affect_identity() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<\n\
          {{ a: 1 }} & {{ b: 2 }},\n\
          {{ b: 2 }} & {{ a: 1 }},\n\
          \"EQ\",\n\
          \"DIFF\"\n\
        >;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "IfEquals should treat reordered intersection members as EQ, not just an exact-position match; got: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Object shape (#16095). Unlike the tuple witness above, the object members of
// an intersection are *merged* into a single synthesized object during
// interning, so nothing shaped like an intersection reaches the relation layer
// and the identity guard has to recover the pre-merge members from the merge
// provenance instead. tsc reports an intersection and its flattened object as
// distinct types; these cases pin that, and the controls pin that two
// intersections over the same members stay identical.
// ---------------------------------------------------------------------------

// NOT covered here, and deliberately: `{ a: 1 } & { a: 1 | number }` vs
// `{ a: 1 }`. When the merge result is structurally equal to one of the
// members it *is* that member's interned type, so no distinguishing origin can
// be recorded without making every plain `{ a: 1 }` claim an intersection
// provenance it does not have. tsc reports TS2344 there; tsz still answers EQ.
// Tracked as the remaining half of #16095.

#[test]
fn merged_object_intersection_is_not_identical_to_the_flattened_object() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ a: 1 }} & {{ b: 2 }}, {{ a: 1; b: 2 }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "An intersection and its flattened object are different type nodes to tsc's isTypeIdenticalTo; got: {diags:#?}"
    );
}

#[test]
fn identical_object_intersections_stay_identical() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ a: 1 }} & {{ b: 2 }}, {{ a: 1 }} & {{ b: 2 }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "Two intersections written from the same members must stay EQ; got: {diags:#?}"
    );
}

#[test]
fn distinct_object_intersections_are_not_identical() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ a: 1 }} & {{ b: 2 }}, {{ a: 1 }} & {{ b: 3 }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"DIFF\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "Intersections over different members must stay DIFF; got: {diags:#?}"
    );
}

#[test]
fn plain_objects_are_unaffected_by_the_merge_origin_lookup() {
    let source = format!(
        "{IF_EQUALS_PRELUDE}\n\
        type R = IfEquals<{{ a: 1; b: 2 }}, {{ b: 2; a: 1 }}, \"EQ\", \"DIFF\">;\n\
        const r: R = \"EQ\";\n"
    );
    let diags = check(&source);
    assert!(
        error_codes(&diags).is_empty(),
        "Two plain objects with reordered members carry no merge origin and must stay EQ; got: {diags:#?}"
    );
}
