//! Regression tests for consumers of the binder's `VALUE_MODULE` /
//! `NAMESPACE_MODULE` split (an identifier-named namespace now carries
//! exactly one of the two flags, matching `tsc`'s `getModuleInstanceState`,
//! instead of both unconditionally).
//!
//! Two downstream sites assumed the old "always both bits" behavior:
//!
//! 1. `get_type_from_type_query_flow_sensitive_with_request`
//!    (`state/type_analysis/core_type_query.rs`) fell through to a
//!    "cannot find name" resolution after the general identifier resolver
//!    had already reported the uninstantiated-namespace-as-value diagnostic
//!    (TS2708), double-reporting the same identifier.
//! 2. `namespace_has_value_exports`
//!    (`state/type_analysis/computed_helpers_binding.rs`) required a nested
//!    namespace member to carry *both* `VALUE_MODULE` and `NAMESPACE_MODULE`
//!    to count as providing runtime value — a condition the two mutually
//!    exclusive flags can no longer satisfy, so every namespace with a
//!    nested instantiated namespace lost its structural `typeof` object type
//!    and fell back to `Lazy(DefId)`.

use tsz_checker::context::CheckerOptions;

fn check(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn uninstantiated_namespace_typeof_reports_ts2708_only() {
    // `M` only declares an interface, so it is never instantiated.
    // `tsc` reports exactly one diagnostic here (TS2708); the flow-sensitive
    // type-query path must not additionally fall through to TS2304.
    let diags = check(
        "\
        namespace M { export interface Point { x: number; y: number } }\n\
        var x: typeof M;\n\
    ",
    );
    assert_eq!(codes(&diags), vec![2708], "diags: {diags:?}");
}

#[test]
fn uninstantiated_namespace_via_renamed_binder_reports_ts2708_only() {
    let diags = check(
        "\
        namespace Config { export interface Options { verbose: boolean } }\n\
        var settings: typeof Config;\n\
    ",
    );
    assert_eq!(codes(&diags), vec![2708], "diags: {diags:?}");
}

#[test]
fn nested_instantiated_namespace_member_visible_on_parent_typeof() {
    // `Outer.inst` is instantiated (contains class `C`), so `typeof Outer`
    // must expose `inst` as a structural property. Assigning the bare
    // `Outer` value to a `typeof Outer.inst`-typed variable is missing `C`,
    // so `tsc` reports TS2741 (a *structural* assignability failure) rather
    // than a generic incompatible-type TS2322 — the fallback tsz took while
    // `Outer`'s value type stayed as an unexpanded `Lazy(DefId)`.
    let diags = check(
        "\
        namespace Outer {\n\
        \x20   export namespace inst { export class C {} }\n\
        }\n\
        import alias = Outer.inst;\n\
        var x: typeof alias;\n\
        x = Outer;\n\
    ",
    );
    assert_eq!(codes(&diags), vec![2741], "diags: {diags:?}");
}

#[test]
fn nested_instantiated_namespace_member_readable_as_property() {
    // Same structural shape as above, but exercised through direct property
    // access instead of an assignability failure — `Outer.inst` must resolve
    // to a class-carrying namespace value, not `error`/`any`.
    let diags = check(
        "\
        namespace Outer {\n\
        \x20   export namespace inst { export class C {} }\n\
        }\n\
        var c = new Outer.inst.C();\n\
        c satisfies Outer.inst.C;\n\
    ",
    );
    assert!(diags.is_empty(), "diags: {diags:?}");
}

// A merged `namespace Bar { export var x }` (unexported) + `export interface
// Bar { y }` inside a parent namespace reproduces the same TS2339-on-parent
// shape ("Property 'Bar' does not exist on type 'typeof Foo'") through the
// full `tsz` CLI pipeline, oracle-verified against `tsc` 7.0.2 — see PR body.
// It is not covered by a unit test here: neither the no-lib nor the
// `lib.es5`/`lib.es2015` unit harness reproduces the property-access check
// this shape depends on (both report zero diagnostics, unlike the full
// pipeline), a pre-existing harness gap unrelated to this fix.

#[test]
fn enum_namespace_merge_typeof_unaffected() {
    // Enum + namespace merges are handled by a separate branch
    // (`is_merged_enum_namespace`) and must stay unaffected by the
    // namespace-only fix above.
    let diags = check(
        "\
        enum E { A, B }\n\
        namespace E { export const extra = \"x\"; }\n\
        var v: string = E.extra;\n\
    ",
    );
    assert!(diags.is_empty(), "diags: {diags:?}");
}

#[test]
fn unexported_value_only_namespace_still_renders_typeof() {
    // `f`'s only member is an unexported `var`, which still instantiates the
    // namespace (VALUE_MODULE). `typeof f` must stay valid.
    let diags = check(
        "\
        namespace f { var hidden = 1; }\n\
        var g: typeof f = f;\n\
    ",
    );
    assert!(diags.is_empty(), "diags: {diags:?}");
}
