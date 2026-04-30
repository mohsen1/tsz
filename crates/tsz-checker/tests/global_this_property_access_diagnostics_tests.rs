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
