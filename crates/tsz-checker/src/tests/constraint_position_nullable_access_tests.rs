//! Constraint-position substitution for generic references with nullable
//! constraints (tsc's `getNarrowableTypeForReference` / `isConstraintPosition`).
//!
//! Structural rule: when a reference to a generic type parameter whose base
//! constraint is a union that includes `undefined`/`null` appears in a
//! constraint position — the object of a property/element access or the target
//! of a call/new — it is seen as its base constraint, so the access is
//! `possibly undefined` (TS18048, or TS2722 for an invoked target) before a
//! guard and narrowed afterwards. The `obj[key]` element access keeps its
//! deferred `T[K]` form (no diagnostic) when the object is a generic type
//! *without* a nullable constraint and the index is a generic index type.

use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Reported repro (#10465): TableBaseEnum with mutually-referential class type
// parameters. The two element accesses before the guard are possibly
// undefined; the two after the guard are not. Uses the lib `Record` to stay
// faithful to the conformance source.
// ---------------------------------------------------------------------------

#[test]
fn reported_repro_mutual_class_type_params_report_before_guard_only() {
    let libs = crate::test_utils::load_lib_files(&["es5.d.ts"]);
    let diags = crate::test_utils::check_source_with_libs(
        r#"
class TableBaseEnum<
    PublicSpec extends Record<keyof InternalSpec, any>,
    InternalSpec extends Record<keyof PublicSpec, any> | undefined = undefined> {
    m() {
        let iSpec = null! as InternalSpec;
        iSpec[null! as keyof InternalSpec];
        iSpec[null! as keyof PublicSpec];
        if (iSpec === undefined) {
            return;
        }
        iSpec[null! as keyof InternalSpec];
        iSpec[null! as keyof PublicSpec];
    }
}
"#,
        "test.ts",
        opts(),
        &libs,
    );
    let n = diags.iter().filter(|d| d.code == 18048).count();
    assert_eq!(
        n,
        2,
        "expected exactly the two pre-guard accesses to be possibly-undefined; got {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Element access on a generic param with a nullable constraint — the core
// positive rule, across parameter / let-binding and two name choices. Inline
// index-signature objects keep these hermetic (no lib required).
// ---------------------------------------------------------------------------

#[test]
fn element_access_on_nullable_generic_param_parameter() {
    let source = r#"
function f<T extends { [k: string]: any } | undefined, K extends keyof T>(obj: T, key: K) {
    obj[key];
}
"#;
    assert_eq!(count(source, 18048), 1, "got {:?}", codes(source));
}

#[test]
fn element_access_on_nullable_generic_param_renamed_bindings() {
    // Same rule with P/X instead of T/K — proves the fix is not keyed on names.
    let source = r#"
function f<P extends { [k: string]: any } | undefined, X extends keyof P>(o: P, k: X) {
    o[k];
}
"#;
    assert_eq!(count(source, 18048), 1, "got {:?}", codes(source));
}

#[test]
fn element_access_on_nullable_generic_param_let_binding() {
    let source = r#"
function g<T extends { [k: string]: any } | undefined>() {
    let v = null! as T;
    v[null! as keyof T];
}
"#;
    assert_eq!(count(source, 18048), 1, "got {:?}", codes(source));
}

// ---------------------------------------------------------------------------
// Property access and call positions are also constraint positions.
// ---------------------------------------------------------------------------

#[test]
fn property_access_on_nullable_generic_param_is_possibly_undefined() {
    let source = r#"
function f<T extends { a: number } | undefined>(x: T) {
    x.a;
}
"#;
    assert_eq!(count(source, 18048), 1, "got {:?}", codes(source));
}

#[test]
fn call_target_on_nullable_generic_param_is_possibly_undefined() {
    // An invoked possibly-undefined target reports TS2722, not TS18048.
    let source = r#"
function f<T extends (() => void) | undefined>(fn: T) {
    fn();
}
"#;
    assert_eq!(count(source, 2722), 1, "got {:?}", codes(source));
}

// ---------------------------------------------------------------------------
// Guard narrows the constraint: no diagnostic after `=== undefined` return.
// ---------------------------------------------------------------------------

#[test]
fn no_diagnostic_after_undefined_guard() {
    let source = r#"
function f<T extends { a: number } | undefined>(x: T) {
    if (x === undefined) {
        return;
    }
    x.a;
}
"#;
    assert_eq!(count(source, 18048), 0, "got {:?}", codes(source));
}

// ---------------------------------------------------------------------------
// Negative / fallback cases.
// ---------------------------------------------------------------------------

#[test]
fn no_diagnostic_for_deferred_indexed_access_without_nullable_constraint() {
    // tsc: `obj[key]` keeps the deferred `T[K]` form (the index is a generic
    // index type and `T` has no nullable constraint), so no TS18048.
    let source = r#"
function f1<T, K extends keyof T>(obj: T, key: K) {
    obj[key];
}
function f2<T extends { [k: string]: string }, K extends keyof T>(obj: T, key: K) {
    obj[key];
}
"#;
    assert_eq!(count(source, 18048), 0, "got {:?}", codes(source));
}

#[test]
fn no_diagnostic_when_constraint_is_not_nullable() {
    // A union constraint without a nullable member is substituted, but it is
    // not possibly-undefined, so property access produces no TS18048.
    let source = r#"
function f<T extends { a: number } | { a: string }>(x: T) {
    x.a;
}
"#;
    assert_eq!(count(source, 18048), 0, "got {:?}", codes(source));
}
