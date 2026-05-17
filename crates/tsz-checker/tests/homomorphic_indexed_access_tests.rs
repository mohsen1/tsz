//! Tests for homomorphic mapped type indexed access correctness.
//!
//! Structural rule: for any homomorphic mapped type `H<T>` over `T`,
//! `keyof H<T>` = `keyof T`. Therefore any `K in keyof T` is also a
//! valid index for `H<T>`. The checker must not emit TS2536 for `H<T>[K]`
//! in such contexts.
//!
//! Covers: Required, Partial, Readonly, user-defined homomorphic types,
//! various modifier combinations (-?, +?, -readonly), and nested patterns.

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

fn ts2536(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2536).collect()
}

// ──────────────────────────────────────────────────────────────────────────
// Standard lib utilities
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn required_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Required<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn partial_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Partial<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Partial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn readonly_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Readonly<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Readonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// ObjectEntries pattern (the secondary case from issue #6616)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn object_entries_pattern_no_ts2536() {
    let diags =
        check_es5("type ObjectEntries<T> = { [K in keyof T]-?: [K, Required<T>[K]] }[keyof T];");
    assert!(
        ts2536(&diags).is_empty(),
        "ObjectEntries pattern must not emit TS2536: {diags:?}"
    );
}

#[test]
fn object_entries_concrete_use_no_errors() {
    let diags = check_es5(
        r#"type ObjectEntries<T> = { [K in keyof T]-?: [K, Required<T>[K]] }[keyof T];
type Obj = { a: number; b?: string };
type OE = ObjectEntries<Obj>;"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "ObjectEntries concrete use must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Modifier variants (-?, +?, -readonly)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn required_t_k_with_remove_optional_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]-?: Required<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] with -? modifier must not emit TS2536: {diags:?}"
    );
}

#[test]
fn partial_t_k_with_add_optional_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]+?: Partial<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Partial<T>[K] with +? modifier must not emit TS2536: {diags:?}"
    );
}

#[test]
fn readonly_t_k_with_remove_readonly_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { -readonly [K in keyof T]: Readonly<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Readonly<T>[K] with -readonly modifier must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// User-defined homomorphic utilities (renamed parameter)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn user_defined_required_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyReq<T> = { [P in keyof T]-?: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyReq<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyReq<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn user_defined_partial_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyPartial<T> = { [P in keyof T]?: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyPartial<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyPartial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn user_defined_readonly_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyReadonly<T> = { readonly [P in keyof T]: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyReadonly<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyReadonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Renamed iteration var: outer K, inner P — must be independent names.
#[test]
fn different_iteration_var_names_no_ts2536() {
    let diags = check_es5(
        "type MyMap<T> = { readonly [Q in keyof T]: T[Q] };\n\
         type Test<T> = { [J in keyof T]: MyMap<T>[J] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Renamed vars Q/J must not emit TS2536 when T is the same: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Correct results (regression guard)
// ──────────────────────────────────────────────────────────────────────────

/// After removing optionality via Required, b? becomes required b: string.
#[test]
fn required_t_k_resolves_correctly() {
    let diags = check_es5(
        r#"type Test<T> = { [K in keyof T]: Required<T>[K] };
type Obj = { a: number; b?: string };
type T1 = Test<Obj>;
const t1: T1 = { a: 1, b: 'x' };"#,
    );
    assert!(
        diags.is_empty(),
        "Required<T>[K] should resolve to correct type without errors: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Negative cases: TS2536 must still be emitted for unrelated key spaces
// ──────────────────────────────────────────────────────────────────────────

/// K extends keyof A must NOT index B when B ≠ A.
#[test]
fn unrelated_keyof_still_emits_ts2536() {
    let diags = check_es5(
        "interface A { x: number; }\n\
         interface B { y: string; }\n\
         type Test<K extends keyof A> = B[K];",
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "B[K] where K extends keyof A but B ≠ A must emit TS2536: {diags:?}"
    );
}

/// Local user-defined Required with different shape must still emit TS2536
/// (regression from `required_constraint_local_alias_tests`).
#[test]
fn local_required_unrelated_shape_emits_ts2536() {
    let diags = check_es5(
        "type Required<T> = { marker: string };\n\
         type Test<T> = { [K in keyof T]: Required<T>[K] };",
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "Local Required with unrelated body must still emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Recursive conditional utility types (DeepRequired / DeepPartial patterns)
//
// Structural rule: when a generic alias body is `T extends C ? A : B` and
// each branch shares the source argument's key space (identity, or a
// non-remapped mapped type whose constraint is `keyof T`), `keyof F<T>` =
// `keyof T`. So `F<T>[K]` with `K in keyof T` must not emit TS2536.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn deep_required_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepRequired<T> = T extends object ? { [P in keyof T]-?: DeepRequired<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepRequired<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepRequired<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_partial_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepPartial<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepPartial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_readonly_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepReadonly<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepReadonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Renamed parameters prove the rule is structural, not keyed on identifier spelling.
#[test]
fn deep_required_renamed_param_no_ts2536() {
    let diags = check_es5(
        "type DeepReq<U> = U extends object ? { [Q in keyof U]-?: DeepReq<U[Q]> } : U;\n\
         type Test<V> = { [J in keyof V]: DeepReq<V>[J] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepReq<V>[J] with renamed params must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_required_concrete_no_errors() {
    let diags = check_es5(
        r#"type DeepRequired<T> = T extends object ? { [P in keyof T]-?: DeepRequired<T[P]> } : T;
type Test<T> = { [K in keyof T]: DeepRequired<T>[K] };
type Obj = { a: number; b?: string };
type Result = Test<Obj>;"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Concrete DeepRequired use must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Deferred conditional types — tsc defers TS2536 to instantiation time
// when the object type is a generic conditional (e.g. `T extends C ? A : B`
// with A's keyof not provably equal to `keyof T`).
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn conditional_deferred_unrelated_branch_no_ts2536_at_generic_level() {
    let diags = check_es5(
        "type Fixed<T> = T extends object ? { x: number } : T;\n\
         type Test<T> = { [K in keyof T]: Fixed<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Fixed<T>[K] with generic T must not emit TS2536 at the generic level (tsc defers): {diags:?}"
    );
}
