//! Regression coverage for issue #9724.
//!
//! Structural rule: a homomorphic mapped type `{ [K in keyof T]: T[K] }` whose
//! source `T` has no enumerable string/number/symbol keys — a bare function or
//! constructor type, or anything whose `keyof` is `never` — has no members and
//! reduces to the empty object type `{}`. `{}` is assignable to it, and the type
//! exposes no members (`r.length` is TS2339) and is not callable (`r()` is
//! TS2349), matching `tsc`.
//!
//! These tests pin tsc parity so the previously-observed false-positive TS2322
//! ("Type '{}' is not assignable to type 'M<() => number>'") cannot regress.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_es5(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn has(diags: &[Diagnostic], c: u32) -> bool {
    diags.iter().any(|d| d.code == c)
}

// ──────────────────────────────────────────────────────────────────────────
// Reported repro and its immediate variants.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn empty_object_assignable_to_mapped_over_function() {
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type R = M<() => number>;\n\
         declare let r: R;\n\
         r = {};",
    );
    assert!(
        !has(&diags, 2322) && !has(&diags, 2741),
        "M<() => number> must accept {{}} (reduces to {{}}): {diags:?}"
    );
}

#[test]
fn empty_object_assignable_to_mapped_over_constructor() {
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type R = M<new () => object>;\n\
         declare let r: R;\n\
         r = {};",
    );
    assert!(
        !has(&diags, 2322) && !has(&diags, 2741),
        "M<new () => object> must accept {{}}: {diags:?}"
    );
}

#[test]
fn rule_is_not_bound_to_the_iteration_variable_name() {
    // Renaming the mapped iteration variable must not change the answer.
    let diags = check_es5(
        "type M<T> = { [Prop in keyof T]: T[Prop] };\n\
         type R = M<() => number>;\n\
         declare let r: R;\n\
         r = {};",
    );
    assert!(
        !has(&diags, 2322) && !has(&diags, 2741),
        "renamed iteration variable must still accept {{}}: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// The reduced type is genuinely empty: no members, not callable.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn reduced_type_has_no_members() {
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type R = M<() => number>;\n\
         declare let r: R;\n\
         r.length;",
    );
    assert!(
        has(&diags, 2339),
        "member access on the empty-reduced mapped type must emit TS2339: {diags:?}"
    );
}

#[test]
fn reduced_type_is_not_callable() {
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type R = M<() => number>;\n\
         declare let r: R;\n\
         r();",
    );
    assert!(
        has(&diags, 2349),
        "call on the empty-reduced mapped type must emit TS2349: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Negative controls: real keys must still be enforced.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn mapped_over_nonempty_object_still_rejects_empty() {
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type R = M<{ a: number }>;\n\
         declare let r: R;\n\
         r = {};",
    );
    assert!(
        has(&diags, 2741) || has(&diags, 2739),
        "mapped type with a required member must reject {{}}: {diags:?}"
    );
}

#[test]
fn mapped_over_function_intersection_with_props_rejects_empty() {
    // A function intersected with an object has real keys (`tag`), so the mapped
    // type is not empty and must reject `{}`.
    let diags = check_es5(
        "type M<T> = { [K in keyof T]: T[K] };\n\
         type FnWith = (() => number) & { tag: string };\n\
         type R = M<FnWith>;\n\
         declare let r: R;\n\
         r = {};",
    );
    assert!(
        has(&diags, 2741) || has(&diags, 2739),
        "mapped type over function-with-properties must reject {{}}: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Standard-library homomorphic wrappers over a function source.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn partial_and_readonly_over_function_accept_empty() {
    let diags = check_es5(
        "type Fn = () => number;\n\
         declare let p: Partial<Fn>;\n\
         declare let ro: Readonly<Fn>;\n\
         p = {};\n\
         ro = {};",
    );
    assert!(
        !has(&diags, 2322) && !has(&diags, 2741),
        "Partial/Readonly over a function source must accept {{}}: {diags:?}"
    );
}
