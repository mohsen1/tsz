//! Member access through a user-declared global `Array<T>` interface under
//! `--noLib`.
//!
//! When lib files are not loaded, the program supplies its own ambient global
//! declarations (`interface Array<T> { ... }`). tsc treats that interface as the
//! apparent type of every array-like value, so its members resolve everywhere a
//! built-in `Array` member would — including on each constituent of a union of
//! array-likes (`number[] | string[]`, `[number] | [string]`).
//!
//! tsz registered the boxed/`Array` base types only when lib was loaded, so
//! under `--noLib` `get_array_base_type()` stayed unset. A bare receiver still
//! resolved methods through a checker-level apparent-type rescue, but the
//! solver's per-union-member path (`resolve_array_property`) had no base to
//! synthesize `Array<T>` from and reported `PropertyNotFound` — collapsing the
//! whole union to a false `TS2339` plus a cascading `TS7006` on the (now
//! un-contextually-typed) callback parameter.
//!
//! The fix registers whichever of these globals the program declares even under
//! `--noLib`, keyed on the fixed built-in name set rather than any user-chosen
//! identifier. Binder names below are deliberately varied (the element type and
//! the array-like spellings differ across cases) so the assertions check the
//! structural rule, not a specific spelling. See issue #15087.

use tsz_common::options::checker::CheckerOptions;

/// Minimal global environment a `--noLib` program must supply for array-like
/// values: a generic `Array<T>` with one method and one data property, plus the
/// other ambient globals the binder expects to exist.
const MINIMAL_GLOBALS: &str = r#"
interface Array<T> { every(p: (v: T) => boolean): boolean; length: number; }
interface Boolean {} interface Number {} interface String {} interface Object {}
interface Function {} interface IArguments {} interface RegExp {}
interface CallableFunction {} interface NewableFunction {}
"#;

fn nolib_diagnostics(body: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        no_lib: true,
        ..CheckerOptions::default()
    };
    let source = format!("{MINIMAL_GLOBALS}{body}");
    crate::test_utils::check_source(&source, "nolib_array.ts", opts)
}

fn codes(diags: &[crate::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

#[test]
fn array_method_resolves_on_bare_receiver() {
    // Control: a bare array receiver already resolved its method.
    let diags = nolib_diagnostics("declare const a: number[]; a.every(c => true);\n");
    assert!(
        diags.is_empty(),
        "bare array receiver must resolve the global Array method, got {:?}",
        codes(&diags)
    );
}

#[test]
fn array_method_resolves_on_union_of_distinct_element_arrays() {
    // The reported repro: a union of two arrays with distinct element types.
    let diags = nolib_diagnostics("declare const a: number[] | string[]; a.every(c => true);\n");
    assert!(
        diags.is_empty(),
        "union of array members must resolve the global Array method (no TS2339/TS7006), got {:?}",
        codes(&diags)
    );
}

#[test]
fn array_method_resolves_on_union_of_distinct_tuples() {
    let diags = nolib_diagnostics("declare const t: [number] | [string]; t.every(c => true);\n");
    assert!(
        diags.is_empty(),
        "union of tuple members must resolve the global Array method, got {:?}",
        codes(&diags)
    );
}

#[test]
fn array_method_resolves_on_mixed_array_tuple_union() {
    let diags = nolib_diagnostics("declare const m: number[] | [number]; m.every(c => true);\n");
    assert!(
        diags.is_empty(),
        "mixed array/tuple union must resolve the global Array method, got {:?}",
        codes(&diags)
    );
}

#[test]
fn genuinely_missing_method_on_union_still_reports_ts2339() {
    // The fix must not over-silence: a method absent from the user's `Array`
    // declaration is still a missing property on the union.
    let diags = nolib_diagnostics("declare const a: number[] | string[]; a.nope();\n");
    assert!(
        codes(&diags).contains(&2339),
        "a method absent from the user Array interface must still report TS2339, got {:?}",
        codes(&diags)
    );
}

#[test]
fn array_data_property_resolves_on_union() {
    // `.length` already resolved (it is answered before the Array<T> base path);
    // pin it so the fix keeps data-property resolution working on unions too.
    let diags =
        nolib_diagnostics("declare const a: number[] | string[]; const n: number = a.length;\n");
    assert!(
        diags.is_empty(),
        "data property `.length` must resolve on the union, got {:?}",
        codes(&diags)
    );
}
