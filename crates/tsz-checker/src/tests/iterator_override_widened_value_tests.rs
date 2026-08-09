//! Regression tests for #17003: a false-positive `TS2416` on an unannotated
//! `next()` override whose inferred return widens to `{ done: boolean; value:
//! any }`.
//!
//! Structural rule: the checker-only iterator-protocol diagnostic
//! (`iterator_result_return_display_mismatch`) forces a member-override
//! mismatch when a class' `next()` override returns an object with a "broad"
//! `done` against a base whose `next()` returns `IteratorResult<_, unknown>`.
//! But an `any`-typed `value` makes that object assignable to *either*
//! `IteratorResult` arm regardless of `done` (`any` satisfies both `TYield` and
//! `TReturn`), so it is NOT a genuine mismatch — `tsc` accepts it. This is
//! exactly the shape an unannotated iterator override produces under
//! `strictNullChecks: false`: `return { done: true, value: undefined }` widens
//! `value: undefined` to `any`, and `done: true` to `boolean`.
//!
//! Oracle: `typescript@7.0.2`, `--strict false --target esnext --lib esnext`
//! reports no diagnostic on the class; `--strict` reports `TS2416`
//! (`done: boolean`/`true` is a genuine mismatch there). Both directions are
//! pinned below, and the class/binder names are varied so the fix cannot be a
//! name match.

use std::sync::Arc;

use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{
    check_source_with_libs, diagnostic_codes, load_lib_files, non_strict_checker_options,
    strict_checker_options,
};

/// The default lib bundle plus `es2025.iterator` (which declares the global
/// `Iterator` abstract class used in the `extends Iterator<T>` clause). The
/// standard default bundle does not include it.
fn iterator_libs() -> Vec<Arc<LibFile>> {
    let mut names: Vec<&str> = crate::test_utils::DEFAULT_LIB_NAMES.to_vec();
    names.push("es2025.iterator.d.ts");
    load_lib_files(&names)
}

fn codes_with_options(src: &str, options: CheckerOptions) -> Vec<u32> {
    let libs = iterator_libs();
    let diags = check_source_with_libs(src, "test.ts", options, &libs);
    diagnostic_codes(&diags)
}

fn nonstrict_codes(src: &str) -> Vec<u32> {
    codes_with_options(src, non_strict_checker_options())
}

fn strict_codes(src: &str) -> Vec<u32> {
    codes_with_options(src, strict_checker_options())
}

#[test]
fn nonstrict_unannotated_next_override_widened_value_is_clean() {
    // `value: undefined` widens to `any` under non-strict; `tsc` accepts.
    let codes = nonstrict_codes(
        "
class MyIterator extends Iterator<string> {
    next() { return { done: true, value: undefined }; }
}
",
    );
    assert!(
        !codes.contains(&2416),
        "expected no TS2416 on the non-strict widened iterator override, got: {codes:?}"
    );
    // The lib must actually resolve, else the guard is vacuous.
    assert!(
        !codes.contains(&2304) && !codes.contains(&2583),
        "iterator lib left unresolved — guard would be vacuous: {codes:?}"
    );
}

#[test]
fn nonstrict_widened_value_clean_under_renamed_binders() {
    // Same shape, different class name and yield type — the fix is structural,
    // not a name/text match.
    let codes = nonstrict_codes(
        "
class WidgetCursor extends Iterator<number> {
    next() { return { done: true, value: undefined }; }
}
",
    );
    assert!(
        !codes.contains(&2416),
        "expected no TS2416 under renamed binders, got: {codes:?}"
    );
}

#[test]
fn nonstrict_explicit_any_value_is_clean() {
    // An explicit `value: any` is the same acceptable shape without relying on
    // non-strict widening of `undefined`.
    let codes = nonstrict_codes(
        "
declare const anyVal: any;
class ExplicitAnyIter extends Iterator<string> {
    next() { return { done: true as boolean, value: anyVal }; }
}
",
    );
    assert!(
        !codes.contains(&2416),
        "expected no TS2416 for an explicit any-valued iterator override, got: {codes:?}"
    );
}

#[test]
fn strict_unannotated_next_override_still_reports_ts2416() {
    // Parity control: under `strictNullChecks`, `value: undefined` does NOT
    // widen to `any`, so `{ done: true; value: undefined }` (with `done`
    // widened to `boolean`) is a genuine mismatch against
    // `IteratorResult<string, undefined>` — `tsc` reports TS2416. The
    // any-value exemption must not suppress this.
    let codes = strict_codes(
        "
class StrictIter extends Iterator<string> {
    next() { return { done: true, value: undefined }; }
}
",
    );
    assert!(
        codes.contains(&2416),
        "expected TS2416 under strict mode (genuine mismatch), got: {codes:?}"
    );
}

#[test]
fn strict_genuine_named_method_return_mismatch_still_reported() {
    // Unrelated genuine override mismatch must still fire (guards that the
    // any-value exemption is narrowly scoped to the iterator `value` shape).
    let codes = strict_codes(
        "
class NumBase { compute(): number { return 0; } }
class StrSub extends NumBase { compute(): string { return ''; } }
",
    );
    assert!(
        codes.contains(&2416),
        "expected TS2416 for a genuine number->string override mismatch, got: {codes:?}"
    );
}
