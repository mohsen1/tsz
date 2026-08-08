//! Regression tests: property access on `unknown` (bare intrinsic, and an
//! unconstrained generic-call type parameter defaulted to `unknown` for lack
//! of inference candidates) under `strictNullChecks: false`.
//!
//! Structural rule: `tsc`'s `getApparentType` maps a bare `unknown`-typed
//! value to the global `Object` type for property lookup only when
//! `strictNullChecks` is off — the substituted/displayed type stays
//! `unknown` (a genuine miss still names `unknown` in the `TS2339` message,
//! not `{}`). tsz's `IntrinsicKind::Unknown` arm in
//! `resolve_property_access_inner`
//! (`crates/tsz-solver/src/operations/property.rs`) always returned
//! `PropertyAccessResult::IsUnknown` regardless of `strictNullChecks`, so
//! `Object.prototype` members (`toString`, `valueOf`, ...) spuriously
//! reported `TS2339` on any `unknown`-typed value once `strictNullChecks`
//! was off — including a generic call like `declare const a: { <T>(): T };
//! a().toString()`, where inference leaves `T` uninferred and defaults to
//! `TypeId::UNKNOWN`, the same intrinsic. Oracle: pinned tsc 7.0.2.

fn non_strict_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source_non_strict_codes(source)
}

fn strict_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source_strict_codes(source)
}

#[test]
fn bare_unknown_object_prototype_member_clean_without_strict_null_checks() {
    let codes = non_strict_codes("declare const x: unknown;\nx.toString();\n");
    assert!(
        !codes.contains(&2339),
        "toString() on unknown must not report TS2339 without strictNullChecks, got: {codes:?}"
    );
}

#[test]
fn bare_unknown_genuine_miss_still_reports_ts2339_without_strict_null_checks() {
    let codes = non_strict_codes("declare const x: unknown;\nx.nonExistentProp;\n");
    assert_eq!(
        codes,
        vec![2339],
        "a genuine miss on unknown must still report TS2339 without strictNullChecks"
    );
}

#[test]
fn bare_unknown_reports_ts18046_under_strict_null_checks() {
    // Negative control: strictNullChecks on keeps the original (stricter)
    // TS18046 behavior — no TS2339 fallback to Object.prototype members.
    let codes = strict_codes("declare const x: unknown;\nx.toString();\n");
    assert_eq!(codes, vec![18046]);
}

#[test]
fn unconstrained_generic_call_default_object_prototype_member_clean_without_strict_null_checks() {
    // #16554-adjacent false positive: `a()` has no arguments to infer `T`
    // from, so `T` defaults to `unknown`; `.toString()` must still resolve.
    let codes = non_strict_codes(
        r#"
declare const a: { <T>(): T };
var r3: string = a().toString();
"#,
    );
    assert!(
        !codes.contains(&2339),
        "toString() on a defaulted-to-unknown type parameter must not report TS2339 without strictNullChecks, got: {codes:?}"
    );
}

#[test]
fn unconstrained_generic_call_default_genuine_miss_still_reports_ts2339_without_strict_null_checks()
{
    let codes = non_strict_codes(
        r#"
declare const a: { <T>(): T };
var r = a().nonExistentProp;
"#,
    );
    assert_eq!(
        codes,
        vec![2339],
        "a genuine miss on a defaulted-to-unknown type parameter must still report TS2339"
    );
}

#[test]
fn unconstrained_generic_call_default_still_object_prototype_only() {
    // Adjacent case: renamed type-parameter binder must not change the result
    // (rules out a name-keyed shortcut).
    let codes = non_strict_codes(
        r#"
declare const make: { <Result>(): Result };
make().valueOf();
"#,
    );
    assert!(
        !codes.contains(&2339),
        "valueOf() on a renamed-binder defaulted-to-unknown type parameter must not report TS2339, got: {codes:?}"
    );
}
