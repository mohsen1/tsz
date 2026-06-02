//! Union-source missing-property elaboration (TS2322 + nested reason).
//!
//! Structural rule: when a *union* value is assigned to an object target and the
//! first failing union member is rejected because it is missing a required
//! property, tsc keeps the top-level `Type 'A | B' is not assignable to type
//! 'T'` (TS2322) and elaborates *which* member fails and *why*:
//!
//! ```text
//! Type 'A | B' is not assignable to type 'A'.
//!   Property 'a' is missing in type 'B' but required in type 'A'.
//! ```
//!
//! For a member that is missing *several* required properties tsc uses the
//! TS2739-style summary instead:
//!
//! ```text
//! Type 'A | C' is not assignable to type 'A'.
//!   Type 'C' is missing the following properties from type 'A': a, b
//! ```
//!
//! tsz previously surfaced the union-source elaboration only when the failing
//! member bottomed out in a *leaf* relation (`undefined`/literal/intrinsic
//! mismatch); a member that failed because it was missing a property fell
//! through to the bare top-level `TypeMismatch`, hiding the root cause. The
//! `MissingProperty`/`MissingProperties` reasons are self-heading (their message
//! names the member type), so routing them through `UnionSourceMismatch`
//! reproduces tsc's chain without the spurious `Type 'M' is not assignable …`
//! header that richer structural reasons would require.
//!
//! These tests vary the binder names (interface/property/alias spellings) so a
//! fix keyed to a particular spelling would not satisfy them. They assert
//! structurally (the elaboration line names the missing property and that it is
//! *required in type* / *missing the following properties*) rather than
//! depending on the exact member rendering, which is governed by the type
//! printer.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when some TS2322 carries a nested-elaboration line matching `predicate`.
fn ts2322_has_nested<P: Fn(&str) -> bool>(diags: &[Diagnostic], predicate: P) -> bool {
    diags.iter().any(|d| {
        d.code == 2322
            && d.related_information
                .iter()
                .any(|info| predicate(&info.message_text))
    })
}

/// True when some TS2322 carries a nested "Property '<prop>' is missing ... but
/// required in type ..." elaboration line.
fn has_missing_property_elaboration(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    ts2322_has_nested(diags, |msg| {
        msg.contains(&needle) && msg.contains("but required in type")
    })
}

/// True when some TS2322 carries a nested "... is missing the following
/// properties from type ...: a, b" summary line naming every property.
fn has_missing_properties_summary(diags: &[Diagnostic], props: &[&str]) -> bool {
    ts2322_has_nested(diags, |msg| {
        msg.contains("missing the following properties from type")
            && props.iter().all(|p| msg.contains(p))
    })
}

/// Canonical repro: a two-member object union where the second member lacks a
/// property the target requires. The top-level stays TS2322; the nested line
/// must name `a`.
#[test]
fn union_member_missing_single_property_emits_elaboration() {
    let diags = diagnostics(
        r#"
interface A { a: 1 }
interface B { b: 2 }
declare const u: A | B;
const v: A = u;
"#,
    );
    assert!(
        has_missing_property_elaboration(&diags, "a"),
        "expected TS2322 with a `Property 'a' is missing ... required in type` \
         elaboration for the failing union member; got {diags:?}"
    );
}

/// Same rule, different interface and property spellings. A name-hardcoded fix
/// would miss this.
#[test]
fn union_member_missing_single_property_renamed() {
    let diags = diagnostics(
        r#"
interface Alpha { first: "x" }
interface Beta { second: "y" }
declare const value: Alpha | Beta;
const out: Alpha = value;
"#,
    );
    assert!(
        has_missing_property_elaboration(&diags, "first"),
        "renamed binders should still elaborate the missing property; got {diags:?}"
    );
}

/// A member missing *multiple* required properties uses the TS2739-style
/// summary, still attached as a nested elaboration on the TS2322.
#[test]
fn union_member_missing_multiple_properties_emits_summary() {
    let diags = diagnostics(
        r#"
interface Both { a: 1; b: 2 }
interface Other { c: 3 }
declare const u: Both | Other;
const v: Both = u;
"#,
    );
    assert!(
        has_missing_properties_summary(&diags, &["a", "b"]),
        "expected a `missing the following properties ...: a, b` summary for the \
         multi-property failing union member; got {diags:?}"
    );
}

/// The elaboration must survive when the union is produced by a distributive
/// conditional over its members (the kysely #10831 family shape): distribution
/// rebuilds the union, but the per-member missing-property failure must still
/// reach the diagnostic chain.
#[test]
fn distributive_conditional_union_member_missing_property_emits_elaboration() {
    let diags = diagnostics(
        r#"
interface S1 { kind: "a" }
interface S2 { kind: "b"; extra: 1 }
type Distribute<T> = T extends unknown ? T : never;
declare const u: Distribute<S1 | S2>;
const v: S2 = u;
"#,
    );
    assert!(
        has_missing_property_elaboration(&diags, "extra"),
        "distributive-conditional-rebuilt unions should still elaborate the \
         missing member property; got {diags:?}"
    );
}

/// A leaf (primitive) member mixed with an object member keeps elaborating the
/// first failing member — guarding that broadening to `MissingProperty` did not
/// regress the pre-existing leaf-member elaboration.
#[test]
fn union_with_primitive_member_still_elaborates_leaf() {
    let diags = diagnostics(
        r#"
interface Obj { a: 1 }
declare const u: number | Obj;
const v: Obj = u;
"#,
    );
    let elaborated = ts2322_has_nested(&diags, |msg| msg.contains("is not assignable to type"));
    assert!(
        elaborated,
        "primitive union member should still elaborate beneath the union line; got {diags:?}"
    );
}
