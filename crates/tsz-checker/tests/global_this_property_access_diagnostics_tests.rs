//! TS7017 / TS7053 / TS2339 diagnostics for `globalThis` property access in
//! `--noImplicitAny` files.
//!
//! Three related fixes share one structural rule: "the access target is
//! `typeof globalThis`" — whether reached through a `globalThis` identifier
//! or through `this` resolving to global. Both paths bottom out in the same
//! type and tsc emits the same diagnostics for both.
//!
//! 1. **TS7017** ("Element implicitly has an 'any' type because type
//!    `typeof globalThis` has no index signature") fires on member-access
//!    forms (`globalThis.unknown` / `this.unknown` when this is global).
//! 2. **TS7053** ("Element implicitly has an 'any' type because expression
//!    of type `"unknown"` can't be used to index type `typeof globalThis`")
//!    fires on element-access forms (`globalThis['unknown']` /
//!    `this['unknown']` when this is global).
//! 3. **TS2339 receiver display** for `Window & typeof globalThis` (and
//!    other intersection annotations) preserves the user-written form;
//!    without the fix, tsz collapsed the intersection to its first member
//!    in the diagnostic message.

fn count(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn message_for(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> Option<String> {
    diags
        .iter()
        .find(|d| d.code == code)
        .map(|d| d.message_text.clone())
}

/// Member access `globalThis.unknown` under `--noImplicitAny` must emit
/// TS7017. Previously fired only when the receiver was `this` resolving to
/// global; the direct `globalThis` form was silently typed as `any`.
#[test]
fn globalthis_member_access_emits_ts7017() {
    let source = r#"
globalThis.someUnknownProperty
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7017) >= 1,
        "TS7017 must fire for direct globalThis.unknown; got: {diags:#?}"
    );
}

/// Type queries use a separate `typeof` member-access path. It must apply the
/// same direct-`globalThis` TS7017 rule as value-space member access.
#[test]
fn globalthis_type_query_member_access_emits_ts7017() {
    let source = r#"
type T = typeof globalThis.someUnknownProperty;
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7017) >= 1,
        "TS7017 must fire for typeof globalThis.unknown; got: {diags:#?}"
    );
}

#[test]
fn bare_typeof_globalthis_exposes_global_value_surface() {
    let source = r#"
interface ArrayConstructor { isArray(arg: any): boolean; }
declare var Array: ArrayConstructor;
type G = typeof globalThis;
declare const g: G;
const n: number = g.Array;
const bad = g.definitelyMissingOnGlobalThis;
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 2322) >= 1,
        "g.Array should resolve to ArrayConstructor and fail number assignment; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7017) >= 1,
        "missing property on typeof globalThis should stay on TS7017; got: {diags:#?}"
    );
    assert!(
        !diags
            .iter()
            .any(|diag| diag.code == 2339 && diag.message_text.contains("Array")),
        "g.Array must not be reported missing on typeof globalThis; got: {diags:#?}"
    );
}

#[test]
fn typeof_globalthis_indexed_access_missing_key_reports_ts2339_only() {
    let source = r#"
type Missing = (typeof globalThis)["\"ambientModule\""];
"#;
    let diags = check_with_no_implicit_any(source);
    let ts2339: Vec<_> = diags.iter().filter(|diag| diag.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "missing typeof globalThis indexed key should emit one TS2339; got: {diags:#?}"
    );
    assert!(
        ts2339[0]
            .message_text
            .contains("Property '\"ambientModule\"' does not exist on type 'typeof globalThis'"),
        "TS2339 must keep the canonical typeof globalThis receiver; got: {}",
        ts2339[0].message_text
    );
    assert_eq!(
        count(&diags, 2536),
        0,
        "TS2536 should not cascade after the missing globalThis property; got: {diags:#?}"
    );
}

#[test]
fn typeof_globalthis_indexed_access_keeps_declared_namespace_keys_valid() {
    let source = r#"
namespace renamedValue { export var val = 1; }
namespace renamedNamespace { export type typ = 1; }
type ValueOk = (typeof globalThis)["renamedValue"];
type NamespaceOk = globalThis.renamedNamespace.typ;
"#;
    let diags = check_with_no_implicit_any(source);
    assert_eq!(
        count(&diags, 2339) + count(&diags, 2536),
        0,
        "declared global namespace/value keys should not regress; got: {diags:#?}"
    );
}

/// Element access `globalThis['unknown']` under `--noImplicitAny` must emit
/// TS7053. Same rationale as TS7017 above.
#[test]
fn globalthis_element_access_emits_ts7053() {
    let source = r#"
globalThis['someUnknownProperty']
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7053) >= 1,
        "TS7053 must fire for direct globalThis['unknown']; got: {diags:#?}"
    );
}

/// `this`-as-global parity: when `this` resolves to `typeof globalThis`,
/// `this.unknown` and `this['unknown']` must keep emitting TS7017 / TS7053.
/// The fix must broaden, not narrow, the existing behaviour.
#[test]
fn this_as_global_member_and_element_access_diagnostics() {
    let source = r#"
this.someUnknownProperty;
this['someUnknownProperty'];
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7017) >= 1,
        "TS7017 must fire for this.unknown when this is global; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7053) >= 1,
        "TS7053 must fire for this['unknown'] when this is global; got: {diags:#?}"
    );
}

/// Intersection annotation must surface in the TS2339 message. tsz used
/// to collapse the intersection to one member during property-access
/// evaluation, dropping the user-written intersection from the diagnostic.
#[test]
fn intersection_annotation_preserved_in_ts2339() {
    let source = r#"
interface A { aProp: number; }
interface B { bProp: string; }
declare let v: A & B;
v.someUnknownProperty;
"#;
    let diags = check_with_no_implicit_any(source);
    let msg = message_for(&diags, 2339).expect("TS2339 should fire for v.unknown");
    assert!(
        msg.contains("A & B"),
        "TS2339 must preserve the intersection annotation; got: {msg}"
    );
}

/// Anti-hardcoding (§25): the rule is "intersection annotation in the
/// receiver display", not specific to two-member intersections of `A & B`.
/// Re-run with three members of named types.
#[test]
fn three_member_intersection_annotation_preserved_in_ts2339() {
    let source = r#"
interface Foo { foo: number; }
interface Bar { bar: number; }
interface Baz { baz: number; }
declare let v: Foo & Bar & Baz;
v.someUnknownProperty;
"#;
    let diags = check_with_no_implicit_any(source);
    let msg = message_for(&diags, 2339).expect("TS2339 should fire for v.unknown");
    assert!(
        msg.contains("Foo & Bar & Baz") || msg.contains("Foo &"),
        "TS2339 must preserve the multi-member intersection annotation; got: {msg}"
    );
}

/// Reduced intersections still display their reduced form. For impossible
/// intersections, tsc reports `never`, not the unreduced source annotation.
#[test]
fn reduced_never_intersection_receiver_displays_never_not_annotation() {
    let source = r#"
class A { private x: unknown; y?: string; }
class B { private x: unknown; y?: string; }
declare let ab: A & B;
ab.y;
"#;
    let diags = check_with_no_implicit_any(source);
    let msg = message_for(&diags, 2339).expect("TS2339 should fire for ab.y");
    assert!(
        msg.contains("type 'never'") && !msg.contains("A & B"),
        "TS2339 should display the reduced never type, not the source annotation; got: {msg}"
    );
}

/// Negative companion: a *union* receiver that flow-narrowing has reduced
/// to a single member must NOT regress to displaying the original union.
/// The narrowed type is what tsc shows; the source-text annotation bridge
/// is intentionally scoped to intersections only.
#[test]
fn narrowed_union_receiver_displays_picked_member_not_annotation() {
    let source = r#"
class A { a: string = ""; }
class B { b: string = ""; }
function f(x: A | B) {
    if (x instanceof A) {
        x.someUnknownProperty;
    }
}
"#;
    let diags = check_with_no_implicit_any(source);
    let msg = message_for(&diags, 2339).expect("TS2339 should fire for narrowed receiver");
    assert!(
        msg.contains("type 'A'") && !msg.contains("A | B"),
        "TS2339 should display the narrowed 'A', not the original union; got: {msg}"
    );
}

#[test]
fn window_typeof_globalthis_annotation_preserves_later_diagnostics() {
    let source = r#"
interface Window {}
declare let win: Window & typeof globalThis;

win.hi
this.hi
globalThis.hi

win['hi']
this['hi']
globalThis['hi']
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 2339) >= 1,
        "TS2339 must fire for win.hi; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7015) >= 1,
        "TS7015 must fire for win['hi']; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7017) >= 2,
        "TS7017 must fire for this.hi/globalThis.hi; got: {diags:#?}"
    );
    assert!(
        count(&diags, 7053) >= 2,
        "TS7053 must fire for this['hi']/globalThis['hi']; got: {diags:#?}"
    );
}

#[test]
fn global_window_property_access_does_not_report_missing_index_signature() {
    let source = r#"
interface Window {}
declare var window: Window & typeof globalThis;
(() => this.window);
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7041) >= 1,
        "TS7041 should still fire for captured global this; got: {diags:#?}"
    );
    assert_eq!(
        count(&diags, 7017),
        0,
        "this.window is a declared global property, not an implicit-any miss; got: {diags:#?}"
    );
}

fn check_with_no_implicit_any(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    use tsz_checker::context::CheckerOptions;
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            ..Default::default()
        },
    )
}
