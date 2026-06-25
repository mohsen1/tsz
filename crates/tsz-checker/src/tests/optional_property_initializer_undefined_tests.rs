//! Regression tests: a class field declared optional and initialized at the
//! declaration site (`prop?: T = undefined`) must be accepted under
//! `strictNullChecks` without `exactOptionalPropertyTypes`.
//!
//! Root cause (issue #14737): the declaration-site initializer assignability
//! check compared the initializer against the bare annotation type `T`, never
//! augmenting it with `| undefined` for an optional property. So
//! `prop?: T = undefined` fired a false TS2322 even though tsc accepts it (an
//! optional property's value type implicitly includes `undefined`). This is the
//! declaration-site counterpart to #10749, which fixed the write/assignment
//! path (`this.#x = undefined`) but not the initializer path.
//!
//! Structural rule: when a class property is declared optional under
//! `strictNullChecks`, the value type its initializer (and assignment) is
//! checked against is `T | undefined` — unless `exactOptionalPropertyTypes` is
//! on, in which case it stays `T` and `= undefined` is correctly rejected.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_codes, check_with_options};

fn codes_exact_optional(src: &str) -> Vec<u32> {
    let diags: Vec<Diagnostic> = check_with_options(
        src,
        CheckerOptions {
            exact_optional_property_types: true,
            ..CheckerOptions::default()
        },
    );
    diags.into_iter().map(|d| d.code).collect()
}

// ---------------------------------------------------------------------------
// #14737: false TS2322 on `prop?: T = undefined` at the declaration site
// ---------------------------------------------------------------------------

#[test]
fn no_false_2322_optional_string_initialized_to_undefined() {
    let c = check_source_codes(
        "
class Router {
    plain?: string = undefined;
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn no_false_2322_optional_boolean_initialized_to_undefined() {
    let c = check_source_codes(
        "
class Router {
    isViewTransitionTypesSupported?: boolean = undefined;
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn no_false_2322_optional_union_initialized_to_undefined() {
    // Mirrors tanstack-router router-core/src/router.ts: an optional field with
    // a union annotation initialized to `undefined`.
    let c = check_source_codes(
        "
interface ViewTransitionOptions { types: string[] }
class Router {
    shouldViewTransition?: boolean | ViewTransitionOptions = undefined;
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn no_false_2322_optional_object_initialized_to_undefined() {
    let c = check_source_codes(
        "
interface Box { value: number; }
class Holder {
    b?: Box = undefined;
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn optional_initializer_undefined_matches_renamed_field() {
    // Different field spelling proves the rule is structural, not name-based.
    let a = check_source_codes("class A { foo?: number = undefined; }");
    let b = check_source_codes("class B { somethingEntirelyDifferent?: number = undefined; }");
    assert!(!a.contains(&2322), "unexpected TS2322 for `foo`: {a:?}");
    assert!(
        !b.contains(&2322),
        "unexpected TS2322 for renamed field: {b:?}"
    );
}

#[test]
fn optional_initializer_to_undefined_valued_expression_is_accepted() {
    // Broader than the `= undefined` literal: any `T | undefined`-valued
    // initializer flows into an optional property without TS2322.
    let c = check_source_codes(
        "
declare const maybe: number | undefined;
class A {
    n?: number = maybe;
}
",
    );
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

#[test]
fn optional_initializer_matches_private_field_baseline() {
    // The write path (`this.#p = undefined`) already accepted `undefined`
    // (#10749); the declaration-site initializer must behave identically.
    let init = check_source_codes("class A { p?: string = undefined; }");
    let write = check_source_codes(
        "
class A {
    #p?: string;
    m(): void { this.#p = undefined; }
}
",
    );
    assert!(
        !write.contains(&2322),
        "private write baseline regressed: {write:?}"
    );
    assert!(
        !init.contains(&2322),
        "initializer differs from write path: {init:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback cases that must keep erroring
// ---------------------------------------------------------------------------

#[test]
fn non_optional_field_initialized_to_undefined_still_errors() {
    let c = check_source_codes("class A { c: number = undefined; }");
    assert!(
        c.contains(&2322),
        "expected TS2322 on non-optional `= undefined`. Got: {c:?}"
    );
}

#[test]
fn optional_field_initialized_to_wrong_value_still_errors() {
    // Optionality adds `undefined`, not `any`: a genuinely wrong initializer
    // value must still be rejected.
    let c = check_source_codes("class A { d?: number = 'x'; }");
    assert!(
        c.contains(&2322),
        "expected TS2322 on optional wrong-typed initializer. Got: {c:?}"
    );
}

#[test]
fn optional_field_initialized_to_matching_value_is_accepted() {
    let c = check_source_codes("class A { b?: number = 5; }");
    assert!(
        !c.contains(&2322),
        "unexpected TS2322 on `b?: number = 5`. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// exactOptionalPropertyTypes keeps the bare target and still rejects undefined
// ---------------------------------------------------------------------------

#[test]
fn exact_optional_property_types_rejects_undefined_initializer() {
    let c = codes_exact_optional("class A { p?: number = undefined; }");
    assert!(
        c.contains(&2322),
        "expected TS2322 on `= undefined` under exactOptionalPropertyTypes. Got: {c:?}"
    );
}

#[test]
fn exact_optional_property_types_accepts_explicit_undefined_annotation() {
    // An explicit `| undefined` in the annotation accepts `= undefined` even
    // under exactOptionalPropertyTypes (the value type already includes it).
    let c = codes_exact_optional("class A { q?: number | undefined = undefined; }");
    assert!(
        !c.contains(&2322),
        "unexpected TS2322 on explicit `| undefined` annotation. Got: {c:?}"
    );
}
