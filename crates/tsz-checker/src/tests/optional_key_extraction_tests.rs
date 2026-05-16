//! Regression tests for `GetOptionalKeys`-style patterns (issue #6709).
//!
//! Structural rule: when evaluating
//!   `{ [K in keyof T]-?: {} extends Pick<T, K> ? K : never }[keyof T]`
//! for a concrete interface `T`, the result must be the union of optional
//! property key names (not `never`).
//!
//! Root cause: `Pick<T, K>` is a lib-type Application that the first-pass
//! `TypeEnvironment` resolver cannot expand. Previously, `evaluate_conditional`
//! took the false branch (`never`) for every key, producing an all-`never`
//! mapped object and therefore a `never` `IndexAccess` result. The fix defers the
//! conditional when the extends-type is still an unevaluated Application; the
//! second resolver pass (`CheckerContext`) then correctly expands `Pick` and
//! evaluates each key's conditional.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

// ── helpers ──────────────────────────────────────────────────────────────────

fn codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

// ── Core repro (issue #6709) ─────────────────────────────────────────────────

/// The exact minimal repro from issue #6709: assigning `"age"` to
/// `A<Person>[keyof Person]` where A extracts optional keys via
/// `{} extends Pick<T, K>`. tsc accepts this; tsz was emitting TS2322.
#[test]
fn optional_key_extraction_inline_no_error() {
    let codes = strict_codes_with_libs(
        r#"
interface Person { name: string; age?: number }
type A<T> = { [K in keyof T]-?: {} extends Pick<T, K> ? K : never }
const _: A<Person>[keyof Person] = "age"
export {}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "A<Person>[keyof Person] = \"age\" must not emit TS2322, got: {codes:?}"
    );
}

/// Anti-hardcoding: same logic with renamed iteration variable (`P` instead
/// of `K`). Proves the fix is keyed on type structure, not on identifier
/// names.
#[test]
fn optional_key_extraction_renamed_iter_var_no_error() {
    let codes = strict_codes_with_libs(
        r#"
interface Person { name: string; age?: number }
type A<T> = { [P in keyof T]-?: {} extends Pick<T, P> ? P : never }
const _: A<Person>[keyof Person] = "age"
export {}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "renamed iteration variable P must not change the result, got: {codes:?}"
    );
}

/// Anti-hardcoding: same logic with renamed type parameter (`U` instead of `T`).
#[test]
fn optional_key_extraction_renamed_type_param_no_error() {
    let codes = strict_codes_with_libs(
        r#"
interface Obj { x: string; y?: number; z?: boolean }
type GetOpt<U> = { [K in keyof U]-?: {} extends Pick<U, K> ? K : never }
const _: GetOpt<Obj>[keyof Obj] = "y"
const _2: GetOpt<Obj>[keyof Obj] = "z"
export {}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "renamed type parameter U must not change the result, got: {codes:?}"
    );
}

/// Required-only object: all keys evaluate `{} extends Pick<T, K>` to false
/// (Pick<T, K> has a required property), so the result is `never`.
/// Assigning any value to `never` must still produce TS2322.
#[test]
fn optional_key_extraction_all_required_yields_never_error() {
    let codes = strict_codes_with_libs(
        r#"
interface AllRequired { name: string; age: number }
type GetOpt<T> = { [K in keyof T]-?: {} extends Pick<T, K> ? K : never }
const _: GetOpt<AllRequired>[keyof AllRequired] = "name"
export {}
"#,
    );
    assert!(
        codes.contains(&2322),
        "assigning to GetOpt<AllRequired>[keyof AllRequired] (== never) must emit TS2322, got: {codes:?}"
    );
}

/// Multiple optional keys: assigning either optional key name is valid.
#[test]
fn optional_key_extraction_multiple_optional_keys_no_error() {
    let codes = strict_codes_with_libs(
        r#"
interface Shape { x?: number; y?: number; label: string }
type GetOpt<T> = { [K in keyof T]-?: {} extends Pick<T, K> ? K : never }
const a: GetOpt<Shape>[keyof Shape] = "x"
const b: GetOpt<Shape>[keyof Shape] = "y"
export {}
"#,
    );
    assert!(
        !codes.contains(&2322),
        "assigning an optional key name must not emit TS2322, got: {codes:?}"
    );
}

/// Assigning a required key name (which is excluded by the pattern) must error.
#[test]
fn optional_key_extraction_required_key_excluded_errors() {
    let codes = strict_codes_with_libs(
        r#"
interface Shape { x?: number; y?: number; label: string }
type GetOpt<T> = { [K in keyof T]-?: {} extends Pick<T, K> ? K : never }
const _: GetOpt<Shape>[keyof Shape] = "label"
export {}
"#,
    );
    assert!(
        codes.contains(&2322),
        "assigning a required key (\"label\") must emit TS2322, got: {codes:?}"
    );
}
