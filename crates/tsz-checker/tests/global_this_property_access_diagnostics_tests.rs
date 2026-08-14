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

/// A string-literal ambient module (`declare module "x"`) is not part of the
/// global scope: `tsc` records it in a separate ambient-module table, so it
/// never becomes a property of `typeof globalThis`. Indexing the surface by the
/// module name — even though the name *is* bound in the file's locals — must
/// still report TS2339 against the canonical `typeof globalThis` receiver.
/// Before the fix the module surfaced as a `typeof <module>` property, so the
/// access resolved instead of erroring.
#[test]
fn typeof_globalthis_indexed_access_declared_ambient_module_reports_ts2339() {
    let source = r#"
declare module "ambientModule" {
    export const x: number;
}
type Bad = (typeof globalThis)["ambientModule"];
"#;
    let diags = check_with_no_implicit_any(source);
    let ts2339: Vec<_> = diags.iter().filter(|diag| diag.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "indexing typeof globalThis by a declared ambient module name should emit one TS2339; got: {diags:#?}"
    );
    assert!(
        ts2339[0]
            .message_text
            .contains("Property 'ambientModule' does not exist on type 'typeof globalThis'"),
        "TS2339 must use the canonical typeof globalThis receiver; got: {}",
        ts2339[0].message_text
    );
}

/// `tsc` keys ambient modules under their quoted name, so the conformance
/// witness indexes `typeof globalThis` by the *quoted* form `"ambientModule"`.
/// That key must also miss the surface. Renamed module binder to prove the
/// rule is structural, not keyed to a specific module name.
#[test]
fn typeof_globalthis_indexed_access_quoted_declared_module_name_reports_ts2339() {
    let source = r#"
declare module "renamedAmbientMod" {
    export type typ = 1;
    export var val: typ;
}
type Bad = (typeof globalThis)["\"renamedAmbientMod\""];
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        diags.iter().any(|d| d.code == 2339
            && d.message_text
                .contains("does not exist on type 'typeof globalThis'")),
        "quoted ambient module name must miss the typeof globalThis surface; got: {diags:#?}"
    );
}

/// Companion: a string-literal ambient module and an identifier value
/// namespace declared side by side. The ambient module is excluded; the value
/// namespace stays a genuine global and indexes cleanly. Guards against the
/// exclusion over-reaching to identifier namespaces.
#[test]
fn typeof_globalthis_surface_excludes_ambient_module_but_keeps_value_namespace() {
    let source = r#"
declare module "someAmbientMod" {
    export const x: number;
}
namespace keptValueNs { export var v = 1; }
type Bad = (typeof globalThis)["someAmbientMod"];
type Ok = (typeof globalThis)["keptValueNs"];
"#;
    let diags = check_with_no_implicit_any(source);
    let ts2339: Vec<_> = diags.iter().filter(|diag| diag.code == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "only the ambient module key should miss the surface; the value namespace stays; got: {diags:#?}"
    );
    assert!(
        ts2339[0].message_text.contains("someAmbientMod"),
        "the surviving TS2339 must be the ambient module, not the value namespace; got: {}",
        ts2339[0].message_text
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

/// `checkJs` files get the same `typeof globalThis` implicit-any rule as
/// `.ts` files: dot access on global `this`/`globalThis` for a missing
/// member reports TS7017, whether the access is a write (`this.x = ...`,
/// the salsa `nestedPrototypeAssignment`-adjacent shape) or a bare read.
#[test]
fn checkjs_global_this_dot_write_emits_ts7017() {
    let source = r#"
this.someUnknownProperty = 1;
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7017) >= 1,
        "TS7017 must fire for this.x = ... in a checkJs file; got: {diags:#?}"
    );
    assert_eq!(
        count(&diags, 2339),
        0,
        "must not fall back to TS2339 in a checkJs file; got: {diags:#?}"
    );
}

#[test]
fn checkjs_global_this_dot_read_emits_ts7017() {
    let source = r#"
this.someUnknownProperty;
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7017) >= 1,
        "TS7017 must fire for this.x in a checkJs file; got: {diags:#?}"
    );
}

/// Negative control: an `allowJs` file WITHOUT `checkJs` is not
/// type-checked at all, so it must stay silent — same as tsc.
#[test]
fn js_without_checkjs_stays_silent() {
    let source = r#"
this.someUnknownProperty = 1;
"#;
    let diags = tsz_checker::test_utils::check_source(
        source,
        "test.js",
        tsz_checker::context::CheckerOptions {
            no_implicit_any: true,
            ..Default::default()
        },
    );
    assert_eq!(
        count(&diags, 7017),
        0,
        "an allowJs-only file (no checkJs) must not be type-checked; got: {diags:#?}"
    );
}

/// A `.ts` file gets no JS "declare a new global property" leniency: even
/// the bare `=` assignment target still reports TS7053, matching tsc.
#[test]
fn ts_file_bare_equals_element_write_still_emits_ts7053() {
    let source = r#"
this["someUnknownProperty"] = {};
"#;
    let diags = check_with_no_implicit_any(source);
    assert!(
        count(&diags, 7053) >= 1,
        "TS7053 must fire for this['unknown'] = ... in a .ts file; got: {diags:#?}"
    );
}

/// A checkJs file's bare `this["y"] = value` is tsc's "declare a new global
/// property" leniency: the assignment target itself stays silent.
#[test]
fn checkjs_global_this_bare_equals_element_write_stays_silent() {
    let source = r#"
this["someUnknownProperty"] = {};
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert_eq!(
        count(&diags, 7053),
        0,
        "a bare `this['x'] = ...` declares a new global property in JS and must stay silent; got: {diags:#?}"
    );
}

/// The JS leniency is for the assignment target only — a plain read of the
/// same shape still reports.
#[test]
fn checkjs_global_this_element_read_emits_ts7053() {
    let source = r#"
var q = this["someUnknownProperty"];
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7053) >= 1,
        "a read of this['unknown'] in JS must still emit TS7053; got: {diags:#?}"
    );
}

/// A nested element write (`this["y"]["z"] = 1`) reads `this["y"]` first —
/// that read is not itself the assignment target, so it keeps reporting even
/// though the outer write is the "new global property" shape.
#[test]
fn checkjs_global_this_nested_element_write_emits_ts7053() {
    let source = r#"
this["someUnknownProperty"]["z"] = 1;
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7053) >= 1,
        "the inner read of this['unknown'] must still emit TS7053; got: {diags:#?}"
    );
}

/// A compound assignment reads the current value before writing it, so it
/// does not qualify for the bare-`=` leniency.
#[test]
fn checkjs_global_this_compound_element_write_emits_ts7053() {
    let source = r#"
this["someUnknownProperty"] += 1;
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7053) >= 1,
        "this['unknown'] += ... in JS must still emit TS7053; got: {diags:#?}"
    );
}

/// `++`/`--` read-then-write the same way a compound assignment does.
#[test]
fn checkjs_global_this_element_increment_emits_ts7053() {
    let source = r#"
this["someUnknownProperty"]++;
"#;
    let diags = check_with_no_implicit_any_js(source);
    assert!(
        count(&diags, 7053) >= 1,
        "this['unknown']++ in JS must still emit TS7053; got: {diags:#?}"
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

fn check_with_no_implicit_any_js(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    use tsz_checker::context::CheckerOptions;
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            no_implicit_any: true,
            check_js: true,
            ..Default::default()
        },
    )
}
