//! Union-target missing-property elaboration (TS2322 + nested reason).
//!
//! Structural rule: when a value is assigned to a *union* target and fails to
//! match any member, tsc selects the best-matching constituent
//! (`getBestMatchingType` -> `findMostOverlappyType`: the member sharing the
//! most property-name keys with the source, ties broken by the *last* such
//! member) and, when that member fails through a missing required property,
//! elaborates the top-level `Type 'S' is not assignable to type '<union>'`
//! (TS2322) with which property of which member is missing:
//!
//! ```text
//! Type 'S' is not assignable to type '<union>'.
//!   Property 'X' is missing in type 'S' but required in type '<member>'.
//! ```
//!
//! tsz previously emitted only the bare top-level TS2322 for union targets,
//! hiding the root property mismatch. See issue #10915
//! (`utility-types-project: Object literals lose missing property names in
//! mapped union messages`).
//!
//! Property *type* mismatches on non-fresh sources elaborate beneath a member
//! frame (see `union_target_property_mismatch_elaboration_tests`); fresh
//! object literals report excess/mismatched properties at the offending
//! property's location via the object-literal contextual elaboration. These
//! tests assert the *missing-property* family only.
//!
//! Tests vary the mapped-type iteration variable, the property names, and the
//! alias/interface names so a fix keyed to a particular spelling would not
//! satisfy them. They assert structurally rather than depending on exact member
//! rendering, except where the member is a named/plain type whose printing is
//! stable.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when some TS2322 carries a nested "Property '<prop>' is missing ... but
/// required in type ..." elaboration line.
fn has_missing_member_elaboration(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains(&needle)
                    && info.message_text.contains("but required in type")
            })
    })
}

/// True when some TS2322 carries a nested grouped "Type 'S' is missing the
/// following properties from type '<member>': ..." elaboration listing every
/// property in `props` (the form tsc uses when one member is missing several
/// required properties).
fn has_grouped_member_elaboration(diags: &[Diagnostic], props: &[&str]) -> bool {
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text
                    .contains("is missing the following properties from type")
                    && props.iter().all(|p| info.message_text.contains(p))
            })
    })
}

/// True when some TS2322 carries a nested missing-property/properties line whose
/// text contains `needle` (used to assert the requiring member and/or the
/// widened source rendering).
fn has_elaboration_containing(diags: &[Diagnostic], needle: &str) -> bool {
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information
                .iter()
                .any(|info| info.message_text.contains(needle))
    })
}

/// Reported repro family: a fresh object literal is assigned to a union of
/// object types and matches none of them. The best member requires one missing
/// property; the nested line must name it and the requiring member.
#[test]
fn single_missing_property_in_union_member_elaborates() {
    let diags = diagnostics(
        r#"
interface AA { a: number; b: string }
interface BB { a: number; c: boolean }
const v: AA | BB = { a: 1 };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "c"),
        "expected TS2322 with a `Property 'c' is missing ... required in type` \
         elaboration for the best-matching union member; got {diags:?}"
    );
    assert!(
        has_elaboration_containing(&diags, "required in type 'BB'"),
        "expected the elaboration to name the requiring member `BB`; got {diags:?}"
    );
}

/// Same rule through type aliases for the union members rather than interfaces.
#[test]
fn single_missing_property_alias_members() {
    let diags = diagnostics(
        r#"
type One = { a: number; b: string };
type Two = { a: number; c: boolean };
const v: One | Two = { a: 1 };
"#,
    );
    assert!(
        has_elaboration_containing(&diags, "required in type 'Two'"),
        "expected `... required in type 'Two'`; got {diags:?}"
    );
}

/// The union-of-mapped-types title case: each member is an identity mapped type
/// over a distinct object. The property name must still be surfaced even when the
/// member display simplifies. Anti-hardcoding: renamed mapped variable.
#[test]
fn union_of_mapped_types_surfaces_property_name() {
    let diags = diagnostics(
        r#"
type Mapped<T> = { [Prop in keyof T]: T[Prop] };
type Left = { a: number; b: string };
type Right = { a: number; c: boolean };
const v: Mapped<Left> | Mapped<Right> = { a: 1 };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "c"),
        "expected the missing-property name `c` to be surfaced for a union of \
         mapped types; got {diags:?}"
    );
}

/// `findMostOverlappyType` tie-break: when two members share the same number of
/// property keys with the source, tsc elaborates the *last* such member. With
/// `shared` common to both, the elaboration must name the second member's
/// `onlySecond`, not the first member's `onlyFirst`.
#[test]
fn best_member_overlap_ties_break_to_last() {
    let diags = diagnostics(
        r#"
interface First { shared: number; onlyFirst: string }
interface Second { shared: number; onlySecond: boolean }
const v: First | Second = { shared: 1 };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "onlySecond")
            && has_elaboration_containing(&diags, "required in type 'Second'"),
        "on an overlap tie tsc elaborates the *last* member (`Second`/`onlySecond`); \
         got {diags:?}"
    );
    assert!(
        !has_missing_member_elaboration(&diags, "onlyFirst"),
        "the earlier tied member (`First`/`onlyFirst`) must not be elaborated on a tie; \
         got {diags:?}"
    );
}

/// `findMostOverlappyType` strict-overlap: the member sharing *more* keys with
/// the source is selected even when it is not last. Here `Wide` shares `aa`
/// while `Narrow` shares nothing, so the grouped `bb, cc` elaboration names
/// `Wide`.
#[test]
fn best_member_higher_overlap_selected_over_last() {
    let diags = diagnostics(
        r#"
interface Wide { aa: number; bb: string; cc: number }
interface Narrow { zz: boolean }
const v: Wide | Narrow = { aa: 1 };
"#,
    );
    assert!(
        has_grouped_member_elaboration(&diags, &["bb", "cc"]),
        "expected the higher-overlap member `Wide` to be elaborated with `bb, cc`; \
         got {diags:?}"
    );
    assert!(
        has_elaboration_containing(&diags, "from type 'Wide'"),
        "the grouped elaboration must name the higher-overlap member `Wide`; got {diags:?}"
    );
}

/// Multiple properties missing from the best member are grouped into one
/// `... is missing the following properties from type '<member>': a, b` line.
/// Anti-hardcoding: renamed members/properties.
#[test]
fn multiple_missing_in_best_member_groups_properties() {
    let diags = diagnostics(
        r#"
interface Alpha { id: number; name: string; tag: number }
interface Beta { z: boolean }
const v: Alpha | Beta = { id: 1 };
"#,
    );
    assert!(
        has_grouped_member_elaboration(&diags, &["name", "tag"]),
        "expected a single grouped `... missing the following properties ... name, tag` \
         elaboration; got {diags:?}"
    );
}

/// A fresh object-literal source must be displayed widened (`{ id: number; }`,
/// not `{ id: 1; }`) in the grouped elaboration, matching tsc. Guards the
/// nested-depth source-widening path.
#[test]
fn fresh_literal_source_is_widened_in_grouped_elaboration() {
    let diags = diagnostics(
        r#"
interface Alpha { id: number; name: string; tag: number }
interface Beta { z: boolean }
const v: Alpha | Beta = { id: 1 };
"#,
    );
    assert!(
        has_elaboration_containing(&diags, "Type '{ id: number; }' is missing"),
        "a fresh object literal source must render widened (`{{ id: number; }}`) in the \
         grouped elaboration, not as the literal `{{ id: 1; }}`; got {diags:?}"
    );
    assert!(
        !has_elaboration_containing(&diags, "{ id: 1; }"),
        "the un-widened literal source must not appear in the elaboration; got {diags:?}"
    );
}

/// A non-literal source (so the object-literal property elaboration cannot fire)
/// still produces the union-member elaboration.
#[test]
fn non_literal_source_emits_elaboration() {
    let diags = diagnostics(
        r#"
interface AA { p: number; q: string }
interface BB { p: number; r: boolean }
declare const src: { p: number };
const v: AA | BB = src;
"#,
    );
    assert!(
        has_elaboration_containing(&diags, "required in type 'BB'"),
        "expected the elaboration for a non-literal source assigned to a union target; \
         got {diags:?}"
    );
}

/// The elaboration also fires when the union target is a *call parameter*
/// (TS2345), not only a declaration/return annotation (TS2322). This exercises
/// the call-site related-information path, which is built separately from the
/// assignment renderer.
#[test]
fn call_argument_to_union_parameter_elaborates() {
    let diags = diagnostics(
        r#"
interface AA { a: number; b: string }
interface BB { a: number; c: boolean }
declare function takes(x: AA | BB): void;
takes({ a: 1 });
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2345
            && d.related_information.iter().any(|info| {
                info.message_text.contains("Property 'c' is missing")
                    && info.message_text.contains("required in type 'BB'")
            })
    });
    assert!(
        matched,
        "expected TS2345 with a `Property 'c' is missing ... required in type 'BB'` \
         elaboration for a union-typed call parameter; got {diags:?}"
    );
}

/// The elaboration also fires in return position against a union return type.
#[test]
fn return_value_to_union_type_elaborates() {
    let diags = diagnostics(
        r#"
interface AA { a: number; b: string }
interface BB { a: number; c: boolean }
function make(): AA | BB { return { a: 1 }; }
"#,
    );
    assert!(
        has_elaboration_containing(&diags, "required in type 'BB'"),
        "expected the union-member elaboration for a returned object literal; got {diags:?}"
    );
}

// ── Negative / fallback cases ────────────────────────────────────────────────

/// A primitive source against a primitive union (`string | number`) yields only
/// the bare top-level TS2322 with no missing-property elaboration, matching tsc.
#[test]
fn primitive_union_has_no_missing_elaboration() {
    let diags = diagnostics(
        r#"
const v: string | number = true;
"#,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2322 && d.message_text.contains("'string | number'")),
        "expected the bare TS2322 for a primitive union mismatch; got {diags:?}"
    );
    assert!(
        diags
            .iter()
            .all(|d| !d.related_information.iter().any(|info| {
                info.message_text.contains("is missing")
                    || info.message_text.contains("but required in type")
            })),
        "a primitive union mismatch must not carry a missing-property elaboration; \
         got {diags:?}"
    );
}

/// A property *type* mismatch in the best member is reported at the property
/// location (object-literal elaboration), not as a missing-member line. Guards
/// against over-eager elaboration.
#[test]
fn property_type_mismatch_is_not_missing_elaboration() {
    let diags = diagnostics(
        r#"
type One = { a: string };
type Two = { b: number };
const v: One | Two = { a: 42 };
"#,
    );
    assert!(
        !has_missing_member_elaboration(&diags, "a")
            && !has_missing_member_elaboration(&diags, "b"),
        "a property *type* mismatch must not be reported as a missing-member elaboration; \
         got {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a TS2322 for the `a` value mismatch; got {diags:?}"
    );
}

/// A complete source that satisfies one union member produces no assignability
/// error at all — and therefore no missing-member elaboration.
#[test]
fn complete_source_has_no_error() {
    let diags = diagnostics(
        r#"
interface AA { a: number; b: string }
interface BB { a: number; c: boolean }
const v: AA | BB = { a: 1, c: true };
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a source matching a union member must not produce an assignability error; \
         got {diags:?}"
    );
}
