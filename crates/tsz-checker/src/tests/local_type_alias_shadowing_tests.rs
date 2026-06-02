//! Regression coverage for lexical-scope resolution of type references.
//!
//! A function- or block-local `type`/`interface` declaration must shadow a
//! same-named top-level declaration when the reference appears as the operand of
//! a `keyof` / `readonly` type operator, inside an indexed access, or inside a
//! mapped-type key space. The lowering previously resolved such references by
//! bare name (file/global scope only), so the local alias was silently bound to
//! the outer declaration — producing false `TS2322` / `TS2536` / `TS2345` /
//! `TS7006` diagnostics for the well-typed correlated-union patterns in
//! `correlatedUnions.ts`.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn strict_codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn keyof_operand_resolves_local_alias_over_top_level() {
    // `keyof T` inside `outer` must use the local `T`, whose keys are
    // "sum" | "concat" — not the top-level `T` (keys "a").
    let codes = strict_codes(
        r#"
type T = { a: number };
function outer() {
    type T = { sum: number; concat: string };
    const ok: keyof T = "sum";
    const bad: keyof T = "a";
}
"#,
    );
    // The only expected error is the deliberate `bad` assignment ("a" is not a
    // key of the local T). No error on the `ok` line.
    assert_eq!(
        codes.iter().filter(|&&c| c == 2322).count(),
        1,
        "expected exactly one TS2322 (for `bad`), got codes: {codes:?}",
    );
}

#[test]
fn keyof_local_alias_without_shadow_is_unaffected() {
    let codes = strict_codes(
        r#"
function outer() {
    type T = { sum: number; concat: string };
    const ok: keyof T = "sum";
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got codes: {codes:?}",
    );
}

#[test]
fn readonly_array_operand_resolves_local_alias() {
    // `readonly T[]` must use the local `T` ({ sum }), so the object literal is
    // valid and no TS2353 excess-property error is produced.
    let codes = strict_codes(
        r#"
type T = { a: number };
function outer() {
    type T = { sum: number };
    const v: readonly T[] = [{ sum: 1 }];
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got codes: {codes:?}",
    );
}

#[test]
fn constrained_index_access_resolves_local_alias() {
    // `K extends Keys` where `Keys = keyof ArgMap` (local). Indexing the local
    // `ArgMap` by `K` and passing concrete keys must type-check even though a
    // same-named top-level `ArgMap` exists. Mirrors the `ff1` block of
    // `correlatedUnions.ts`.
    let codes = strict_codes(
        r#"
type ArgMap = { a: number; b: string };
function f1<K extends keyof ArgMap>(key: K, arg: ArgMap[K]) {}

function outer() {
    type ArgMap = {
        sum: [a: number, b: number];
        concat: [a: string, b: string, c: string];
    };
    type Keys = keyof ArgMap;
    function apply<K extends Keys>(funKey: K, ...args: ArgMap[K]) {}
    apply("sum", 1, 2);
    apply("concat", "s1", "s2", "s3");
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got codes: {codes:?}",
    );
}

#[test]
fn renamed_binders_resolve_local_alias() {
    // Same structural rule with different binder names — guards against any
    // accidental name-literal dependence in the fix.
    let codes = strict_codes(
        r#"
type Registry = { alpha: number };
function scope() {
    type Registry = { beta: string; gamma: boolean };
    const ok: keyof Registry = "beta";
    const v: readonly Registry[] = [{ beta: "x", gamma: true }];
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got codes: {codes:?}",
    );
}

#[test]
fn block_scoped_alias_shadows_in_keyof() {
    // A block-scoped (not just function-scoped) local alias must also win.
    let codes = strict_codes(
        r#"
type T = { a: number };
function outer() {
    {
        type T = { local: number };
        const ok: keyof T = "local";
    }
}
"#,
    );
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got codes: {codes:?}",
    );
}
