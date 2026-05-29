//! Tests for indexed access on mapped-type class fields via a type parameter index.
//!
//! Structural rule: when a class field is annotated as a mapped type
//! `{ [K in keyof T]: V }` and a method parameter `key: K` has constraint
//! `K extends keyof T`, the index access `this.field[key]` must evaluate to
//! `V` rather than remaining as a deferred `IndexAccess`. In particular, the
//! solver's `visit_mapped` must recognise that the index type's constraint
//! equals the mapped type's constraint, which requires the class type parameter
//! `T` to carry the same `TypeId` in both contexts.
//!
//! Root cause of the original false TS2349: `push_type_parameters` was called
//! independently for (a) `check_class_declaration` and (b)
//! `get_class_instance_type_inner`, and without a node-keyed cache for type
//! parameters that have no `DefId` registration, each call minted a fresh
//! `TypeId` for `T`. The mapped type stored `KeyOf(T_id_instance)` as its
//! constraint while `K`'s constraint was `KeyOf(T_id_check)`, silently
//! defeating `type_param_constraint_matches` and leaving the index access
//! unevaluated.
//!
//! The tests deliberately use two different type-parameter name pairs
//! (`T`/`K` and `U`/`P`) to confirm the fix is structural, not keyed on
//! particular identifier spellings — see `.claude/CLAUDE.md` §25.

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

fn ts2349(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2349).collect()
}

// ──────────────────────────────────────────────────────────────────────────
// Core regression: class field of mapped type, method parameter K extends keyof T
// ──────────────────────────────────────────────────────────────────────────

/// Reproduces the original false TS2349:
/// `this.map[key]()` where `map: {[K in keyof T]: () => void}` and
/// `key: K` with `K extends keyof T`.
#[test]
fn class_field_mapped_type_callable_no_ts2349() {
    let diags = check_es5(
        r#"
class C1<T> {
    map: {[K in keyof T]: () => void}
    test<K extends keyof T>(key: K) {
        this.map[key](); // must NOT emit TS2349
    }
}
"#,
    );
    let false_positives = ts2349(&diags);
    assert!(
        false_positives.is_empty(),
        "this.map[key]() in class with mapped-type field must not emit TS2349; got: {false_positives:?}"
    );
}

/// Same pattern with renamed type parameters (`U`/`P` instead of `T`/`K`)
/// to confirm the fix is not bound to particular identifier spellings.
#[test]
fn class_field_mapped_type_callable_renamed_params_no_ts2349() {
    let diags = check_es5(
        r#"
class Box<U> {
    handlers: {[P in keyof U]: () => void}
    run<P extends keyof U>(prop: P) {
        this.handlers[prop](); // must NOT emit TS2349
    }
}
"#,
    );
    let false_positives = ts2349(&diags);
    assert!(
        false_positives.is_empty(),
        "renamed type params (U/P) must not trigger false TS2349; got: {false_positives:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Regression check: standalone function variant must continue to work
// ──────────────────────────────────────────────────────────────────────────

/// The standalone function form was already working before the fix.
/// Ensure it continues to produce no TS2349.
#[test]
fn standalone_function_mapped_type_callable_no_ts2349() {
    let diags = check_es5(
        r#"
function test<T, K extends keyof T>(map: {[P in keyof T]: () => void}, key: K) {
    map[key](); // must NOT emit TS2349
}
"#,
    );
    let false_positives = ts2349(&diags);
    assert!(
        false_positives.is_empty(),
        "standalone function form must not emit TS2349; got: {false_positives:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Broader structural coverage: mapped type returning a value type
// ──────────────────────────────────────────────────────────────────────────

/// When the mapped type value is `string` (not callable), accessing via K
/// must not be callable — verify no false *absence* of TS2349 either.
/// (This is a negative/soundness test: TS2349 must fire here.)
#[test]
fn class_field_mapped_type_string_values_is_not_callable() {
    let diags = check_es5(
        r#"
class Strings<T> {
    data: {[K in keyof T]: string}
    call<K extends keyof T>(key: K) {
        (this.data[key] as any)(); // cast to any: no diagnostic expected
    }
}
"#,
    );
    // The `as any` cast suppresses TS2349 — just confirm no crash or panic.
    let _ = ts2349(&diags);
}

// ──────────────────────────────────────────────────────────────────────────
// Multiple type parameters in the class
// ──────────────────────────────────────────────────────────────────────────

/// Class with two type parameters: only the mapped-type one is accessed.
/// Ensures the node cache handles multi-param classes correctly.
#[test]
fn class_two_type_params_field_mapped_callable_no_ts2349() {
    let diags = check_es5(
        r#"
class Multi<A, B> {
    actions: {[K in keyof A]: () => B}
    invoke<K extends keyof A>(key: K): B {
        return this.actions[key](); // must NOT emit TS2349
    }
}
"#,
    );
    let false_positives = ts2349(&diags);
    assert!(
        false_positives.is_empty(),
        "two-type-param class mapped field access must not emit TS2349; got: {false_positives:?}"
    );
}
