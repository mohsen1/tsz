//! Intersection-target missing-property elaboration (TS2322 + nested reason).
//!
//! Structural rule: when a value is assigned to an *intersection* target and a
//! required property of one intersection member is missing, tsc keeps the
//! top-level `Type 'S' is not assignable to type '<intersection>'` (TS2322) but
//! elaborates *which* member requires the missing property:
//!
//! ```text
//! Type 'S' is not assignable to type '<intersection>'.
//!   Property 'X' is missing in type 'S' but required in type '<member>'.
//! ```
//!
//! tsz previously emitted only the bare top-level TS2322 for intersection
//! targets (the "intersection fallback"), hiding the root property mismatch.
//! See issue #11480 (`checker intersection fallback hides root property mismatch
//! in mapped rows`).
//!
//! These tests vary the mapped-type iteration variable, the property names, and
//! the alias names so a fix keyed to a particular spelling would not satisfy
//! them. They assert structurally (the elaboration line names the missing
//! property and that it is *required in type*) rather than depending on the
//! exact member rendering, which is governed by the type printer.

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
/// property in `props` (the form tsc uses when one intersection member is
/// missing several required properties).
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

/// Reported repro: a mapped member of the intersection requires the missing
/// property. The top-level stays TS2322; the nested line must name `b`.
#[test]
fn missing_property_in_mapped_member_emits_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", c: true };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "b"),
        "expected TS2322 with a `Property 'b' is missing ... required in type` \
         elaboration for the mapped intersection member; got {diags:?}"
    );
}

/// Same rule, different mapped-variable spelling (`P` instead of `K`),
/// different property names, and different alias names. A name-hardcoded fix
/// would miss this.
#[test]
fn missing_property_in_mapped_member_renamed_vars() {
    let diags = diagnostics(
        r#"
type Identity<U> = { [P in keyof U]: U[P] };
type Combined = Identity<{ first: string; second: number }> & { flag: boolean };
const w: Combined = { first: "x", flag: true };
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "second"),
        "expected the elaboration regardless of mapped-variable / property / \
         alias spelling; got {diags:?}"
    );
}

/// The missing property lives in a *plain* (non-mapped) member of the
/// intersection. Here the member rendering is stable, so we can assert the
/// full elaboration text including the requiring member.
#[test]
fn missing_property_in_plain_member_names_member() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string }> & { b: number };
const v: Target = { a: "s" };
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains("Property 'b' is missing")
                    && info
                        .message_text
                        .contains("required in type '{ b: number; }'")
            })
    });
    assert!(
        matched,
        "expected `Property 'b' is missing ... required in type '{{ b: number; }}'`; \
         got {diags:?}"
    );
}

/// A non-literal source (so the object-literal property elaboration cannot fire)
/// still produces the intersection member elaboration.
#[test]
fn missing_property_non_literal_source_emits_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
declare const src: { a: string; c: boolean };
const v: Target = src;
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "b"),
        "expected the elaboration for a non-literal source assigned to an \
         intersection target; got {diags:?}"
    );
}

/// Negative / fallback: when the source satisfies every intersection member,
/// there is no assignability error at all — and therefore no missing-member
/// elaboration. Guards against a spurious diagnostic.
#[test]
fn complete_source_has_no_error() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", b: 1, c: true };
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a complete source must not produce an assignability error; got {diags:?}"
    );
}

/// Negative / fallback: a member property whose *type* mismatches (rather than
/// being missing) is reported at the property location, not as a missing-member
/// elaboration on the intersection chain. Guards against over-eager elaboration.
#[test]
fn member_property_type_mismatch_is_not_missing_elaboration() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string }> & { b: number };
const v: Target = { a: 123, b: 1 };
"#,
    );
    // The `a` mismatch is a value error, not a missing property.
    assert!(
        !has_missing_member_elaboration(&diags, "a"),
        "a property *type* mismatch must not be reported as a missing-member \
         elaboration; got {diags:?}"
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a TS2322 for the `a` value mismatch; got {diags:?}"
    );
}

// ── Multiple-missing-property (MissingProperties) cases ──────────────────────

/// When several properties are missing and all live in the *same* intersection
/// member, tsc groups them into one `... is missing the following properties
/// from type '<member>': a, b, c` line under the top-level TS2322 (rather than
/// one line per property).
#[test]
fn multiple_missing_in_mapped_member_groups_properties() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number; c: boolean }> & { d: string };
const v: Target = { d: "x" };
"#,
    );
    assert!(
        has_grouped_member_elaboration(&diags, &["a", "b", "c"]),
        "expected a single grouped `... missing the following properties ... a, b, c` \
         elaboration on the mapped member; got {diags:?}"
    );
}

/// tsc checks intersection members left-to-right and elaborates only the FIRST
/// member the source fails against. With `x` missing from the mapped member and
/// `y` missing from a later member, only the `x`/mapped-member elaboration is
/// reported.
#[test]
fn multiple_missing_across_members_reports_first_member_only() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ x: string }> & { y: number };
const v: Target = {};
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "x"),
        "expected elaboration for missing `x` in the first (mapped) member; got {diags:?}"
    );
    assert!(
        !has_missing_member_elaboration(&diags, "y")
            && !has_grouped_member_elaboration(&diags, &["y"]),
        "only the first failing member should be elaborated; `y` (from a later member) \
         must not appear; got {diags:?}"
    );
}

/// Anti-hardcoding: different iteration-variable / property / alias spellings
/// must not affect the grouped multi-property elaboration.
#[test]
fn multiple_missing_renamed_vars_groups_properties() {
    let diags = diagnostics(
        r#"
type Wrap<U> = { [Q in keyof U]: U[Q] };
type Combined = Wrap<{ alpha: string; beta: number }> & { gamma: boolean };
const w: Combined = { gamma: true };
"#,
    );
    assert!(
        has_grouped_member_elaboration(&diags, &["alpha", "beta"]),
        "expected the grouped elaboration regardless of mapped-variable / property / \
         alias spelling; got {diags:?}"
    );
}

/// A plain (non-mapped) intersection alias is normalised to an object type by
/// the solver, but the written annotation is still an intersection, so tsc keeps
/// the top-level TS2322 and elaborates the first member member-by-member — not a
/// flat top-level TS2739 against the merged object.
#[test]
fn plain_intersection_alias_multiple_missing_elaborates_member() {
    let diags = diagnostics(
        r#"
type Target = { a: string; b: number } & { c: boolean };
const v: Target = {};
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322) && !diags.iter().any(|d| d.code == 2739),
        "a plain intersection annotation must report TS2322 with a member elaboration, \
         not a flat top-level TS2739; got {diags:?}"
    );
    assert!(
        has_grouped_member_elaboration(&diags, &["a", "b"]),
        "expected the first member `{{ a: string; b: number; }}` to be elaborated with \
         its missing properties `a, b`; got {diags:?}"
    );
}

/// Negative: a complete source must not trigger any missing-member elaboration,
/// even for multi-property intersection targets.
#[test]
fn multiple_missing_complete_source_no_error() {
    let diags = diagnostics(
        r#"
type Map1<T> = { [K in keyof T]: T[K] };
type Target = Map1<{ a: string; b: number }> & { c: boolean };
const v: Target = { a: "s", b: 1, c: true };
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322 || d.code == 2741),
        "a complete source must not produce any assignability error; got {diags:?}"
    );
}

// ── Assignment-expression target & plain-object anti-leak negative ────────────

/// The member elaboration also fires for an assignment *expression*
/// (`target = { ... }`) whose left-hand side was declared with an intersection
/// annotation — not only declaration initializers. This exercises the
/// `BINARY_EXPRESSION` branch of the annotation walk in `target_annotation_node`.
/// Renamed alias/property spellings guard against a name-keyed fix.
#[test]
fn assignment_expression_to_intersection_target_elaborates_member() {
    let diags = diagnostics(
        r#"
type Lhs1 = { alpha: number };
type Lhs2 = { beta: string };
let target: Lhs1 & Lhs2;
target = { alpha: 1 };
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2322
            && d.related_information.iter().any(|info| {
                info.message_text.contains("Property 'beta' is missing")
                    && info.message_text.contains("required in type 'Lhs2'")
            })
    });
    assert!(
        matched,
        "expected TS2322 + `... required in type 'Lhs2'` for an assignment-expression \
         target declared as an intersection; got {diags:?}"
    );
}

/// Anti-leak negative: a PLAIN object-literal target that is structurally
/// identical to a named intersection elsewhere in the program must still report
/// a flat TS2741 with no member elaboration. The merged intersection and the
/// plain object intern to the same `TypeId` (sharing the display alias), so the
/// member-elaboration recovery — which is gated on the *written annotation*
/// being an intersection — must NOT fire here. Renamed spellings guard against a
/// fix keyed to particular names.
#[test]
fn plain_object_target_does_not_leak_intersection_elaboration() {
    let diags = diagnostics(
        r#"
type Leaf1 = { qq: number };
type Leaf2 = { rr: string };
type Merged = Leaf1 & Leaf2;
declare const forcesMerged: Merged;
const plain: { qq: number; rr: string } = { qq: 1 };
"#,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2741 && d.message_text.contains("Property 'rr' is missing")),
        "expected a flat TS2741 for the missing `rr` on the plain object target; got {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "a plain object-literal target must not receive the intersection-member \
         elaboration (TS2322) even when an identically-shaped named intersection \
         exists in the program; got {diags:?}"
    );
    assert!(
        !has_missing_member_elaboration(&diags, "rr"),
        "no `required in type '<member>'` elaboration may leak onto a plain object \
         target; got {diags:?}"
    );
}

/// Regression for `excessPropertyCheckIntersectionWithIndexSignature`: when the
/// intersection target is an index-signature *value* type and the failing
/// source is a nested, contextually-literal object (`{ a: 0 }` checked against
/// `{ a: 0 } & { b: 0 }`), the source must render with its literal preserved
/// (`{ a: 0; }`), not widened to `{ a: number; }`. The genuine-intersection path
/// must not apply the assignment-anchor literal widening that the merged
/// (recovered) path uses for top-level assigned literals.
#[test]
fn nested_index_signature_intersection_keeps_literal_source() {
    let diags = diagnostics(
        r#"
let x: { [k: string]: { a: 0 } } & { [k: string]: { b: 0 } };
x = { y: { a: 0 } };
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2322
            && d.message_text
                .contains("Type '{ a: 0; }' is not assignable")
            && d.message_text.contains("'{ a: 0; } & { b: 0; }'")
    });
    assert!(
        matched,
        "the nested index-signature intersection source must keep its literal type \
         (`{{ a: 0; }}`, not the widened `{{ a: number; }}`); got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Return / value positions. The same recovered-intersection elaboration must
// fire when the assignment target is a function's declared *return type* that
// is written as an intersection. Before this, a `return <obj>` checked against
// an intersection return type fell back to tsz's eagerly-merged single-object
// shape — `required in type '{ d: number; a: number; }'`, a type the user never
// wrote — instead of naming the specific intersection member.
// ---------------------------------------------------------------------------

/// `function g(): A & B { return ... }` — a return value missing a required
/// member property keeps the top-level TS2322 against the written intersection
/// and names the specific member, exactly as the variable-annotation path does.
#[test]
fn missing_property_in_function_return_type_intersection_emits_member_elaboration() {
    let diags = diagnostics(
        r#"
type A = { a: number };
type B = { d: number };
function g(): A & B {
    return { d: 1 };
}
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "a"),
        "return against `A & B` must elaborate the missing member property `a`; got {diags:?}"
    );
    // The member, not tsz's eagerly-merged `{ d: number; a: number; }`, owns the
    // requirement: the nested line must name a single-property member shape.
    let names_merged_shape = diags.iter().any(|d| {
        d.related_information
            .iter()
            .any(|info| info.message_text.contains("{ d: number; a: number; }"))
    });
    assert!(
        !names_merged_shape,
        "the elaboration must not attribute the requirement to a synthesized merged \
         object the user never wrote; got {diags:?}"
    );
}

/// The rule is not keyed to a function *declaration*: an arrow with an
/// intersection return type behaves identically (renamed binders too).
#[test]
fn missing_property_in_arrow_return_type_intersection_emits_member_elaboration() {
    let diags = diagnostics(
        r#"
type Left = { first: number };
type Right = { second: number };
const make = (): Left & Right => {
    return { second: 2 };
};
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "first"),
        "arrow return against `Left & Right` must elaborate the missing member \
         property `first`; got {diags:?}"
    );
}

/// Method declarations with an intersection return type take the same path.
#[test]
fn missing_property_in_method_return_type_intersection_emits_member_elaboration() {
    let diags = diagnostics(
        r#"
type P = { p: number };
type Q = { q: number };
class C {
    m(): P & Q {
        return { q: 0 };
    }
}
"#,
    );
    assert!(
        has_missing_member_elaboration(&diags, "p"),
        "method return against `P & Q` must elaborate the missing member property `p`; \
         got {diags:?}"
    );
}

/// An inline intersection return type echoes the written `&` form at the top
/// level and names the inline member shape, matching tsc.
#[test]
fn inline_intersection_return_type_keeps_written_form_and_member() {
    let diags = diagnostics(
        r#"
function g(): { a: number } & { d: number } {
    return { d: 1 };
}
"#,
    );
    let matched = diags.iter().any(|d| {
        d.code == 2322
            && d.message_text.contains("'{ a: number; } & { d: number; }'")
            && d.related_information.iter().any(|info| {
                info.message_text.contains("Property 'a' is missing")
                    && info
                        .message_text
                        .contains("but required in type '{ a: number; }'")
            })
    });
    assert!(
        matched,
        "inline intersection return type must keep the written `&` form and name the \
         `{{ a: number; }}` member; got {diags:?}"
    );
}

/// Negative guard: a non-intersection return type is unaffected — a plain
/// object return type keeps the ordinary direct TS2741 missing-property surface.
#[test]
fn non_intersection_return_type_keeps_plain_missing_property_surface() {
    let diags = diagnostics(
        r#"
function g(): { a: number } {
    return {};
}
"#,
    );
    let only_2741 = !diags.is_empty() && diags.iter().all(|d| d.code == 2741);
    assert!(
        only_2741,
        "a plain (non-intersection) return type must keep the direct TS2741 surface; \
         got {diags:?}"
    );
}
