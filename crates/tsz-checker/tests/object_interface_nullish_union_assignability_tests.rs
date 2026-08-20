//! `Object` interface assignability rejects a nullish-carrying union source
//! under strict null checks (#17761).
//!
//! Structural rule: `tsc`'s global `Object` interface accepts every
//! non-nullish value (primitives box to their wrapper interfaces), but
//! rejects `null`/`undefined`/`void` under `strictNullChecks` — including
//! when one arrives as a member of a union source (`string | null`). tsz's
//! `CompatChecker`'s `Object`-interface fast path (`relations/compat.rs`)
//! guarded this with `TypeId::is_nullable()`, which only matches the bare
//! `NULL`/`UNDEFINED`/`VOID` intrinsics: a union carrying one of them fell
//! through the guard and was accepted, since the fast path's own
//! `has_conflicting_properties_with_object` check only inspects property
//! shape, not nullish membership. The guard now uses
//! `narrowing::utils::is_nullish_type`, which recurses into unions the same
//! way `tsc`'s `maybeTypeOfKind(source, TypeFlags.Nullable)` does.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn check_with(source: &str, strict: bool) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(
        &[("main.ts", source)],
        "main.ts",
        CheckerOptions {
            strict,
            strict_null_checks: strict,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn messages(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// The witness: a union of `string | null` is not assignable to `Object`
/// under strict null checks (`null` is not a member of `Object`'s domain).
#[test]
fn nullable_union_source_rejected_strict() {
    let diags = check_with(
        r#"
declare var v: string | null;
var x: Object = v;
"#,
        true,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {:#?}",
        messages(&diags)
    );
    assert_eq!(diags[0].code, 2322);
}

/// Renamed binder, reversed union member order: order must not matter.
#[test]
fn nullable_union_source_rejected_strict_reversed_order() {
    let diags = check_with(
        r#"
declare var w: null | string;
var obj: Object = w;
"#,
        true,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {:#?}",
        messages(&diags)
    );
    assert_eq!(diags[0].code, 2322);
}

/// `undefined` and `void` unions are nullish too and must also be rejected.
#[test]
fn undefined_and_void_union_sources_rejected_strict() {
    for (source, context) in [
        (
            r#"
declare var v: number | undefined;
var x: Object = v;
"#,
            "number | undefined",
        ),
        (
            r#"
declare function f(): void;
var x: Object = (Math.random() > 0.5 ? f() : "s");
"#,
            "void | string",
        ),
    ] {
        let diags = check_with(source, true);
        assert_eq!(
            diags.len(),
            1,
            "{context}: expected exactly one diagnostic, got: {:#?}",
            messages(&diags)
        );
        assert_eq!(diags[0].code, 2322, "{context}");
    }
}

/// Negative control: a bare `null`/`undefined` (not a union) was already
/// correctly rejected before this fix and must stay rejected.
#[test]
fn bare_nullish_source_rejected_strict() {
    for (source, context) in [
        ("var x: Object = null;", "bare null"),
        ("var x: Object = undefined;", "bare undefined"),
    ] {
        let diags = check_with(source, true);
        assert_eq!(
            diags.len(),
            1,
            "{context}: expected exactly one diagnostic, got: {:#?}",
            messages(&diags)
        );
        assert_eq!(diags[0].code, 2322, "{context}");
    }
}

/// Negative control: a non-nullish union of primitives is still accepted —
/// the fix must not over-reject unions that carry no nullish member.
#[test]
fn non_nullish_union_source_still_accepted() {
    let diags = check_with(
        r#"
declare var v: string | number;
var x: Object = v;
"#,
        true,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:#?}",
        messages(&diags)
    );
}

/// Negative control: under `--strictNullChecks: false`, `null`/`undefined`
/// (bare or unioned) are still assignable to `Object`, matching non-strict
/// leniency — the fix is scoped to the strict guard only.
#[test]
fn nullable_union_source_accepted_nonstrict() {
    let diags = check_with(
        r#"
declare var v: string | null;
var x: Object = v;
"#,
        false,
    );
    assert!(
        diags.is_empty(),
        "expected no diagnostics (non-strict), got: {:#?}",
        messages(&diags)
    );
}

/// Negative control: an unconstrained generic type parameter is excluded
/// from the `Object` fast path regardless of union-vs-bare shape (an
/// existing, unrelated guard on the same `if`) — must remain unaffected.
#[test]
fn unconstrained_type_parameter_source_rejected_strict() {
    let diags = check_with(
        r#"
function f<T>(v: T): Object {
    return v;
}
"#,
        true,
    );
    assert_eq!(
        diags.len(),
        1,
        "expected exactly one diagnostic, got: {:#?}",
        messages(&diags)
    );
    assert_eq!(diags[0].code, 2322);
}
