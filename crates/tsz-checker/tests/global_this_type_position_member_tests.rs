//! `globalThis.X` in **type** position resolves `globalThis` to the synthetic
//! global namespace and `X` to the ambient global type of that name (e.g.
//! `globalThis.RegExp` is the global `RegExp` interface). A failure to resolve
//! the member dropped the qualified name to `TypeId::ERROR`, which then
//! collapsed whatever the type fed — for a type predicate `value is
//! globalThis.RegExp`, the predicate's false branch collapsed and surfaced a
//! spurious TS2339 on the narrowed value.
//!
//! Regression coverage for #14227 (mined from typebox).

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_common::checker_options::JsxMode;

fn diag_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let libs: Vec<Arc<LibFile>> = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn global_this_type_member_in_type_predicate_is_clean() {
    // The typebox repro: `globalThis.RegExp` / `globalThis.Boolean` in a
    // type-predicate return type must resolve to the global interfaces so the
    // predicate's false branch narrows correctly (no spurious TS2339).
    let source = r#"
function IsRegExp(value: unknown): value is globalThis.RegExp {
  return value instanceof RegExp;
}
function IsBoolean(value: unknown): value is globalThis.Boolean {
  return value instanceof Boolean;
}
function FromValue(value: unknown): void {
  return IsRegExp(value)
    ? void 0
    : IsBoolean(value)
    ? (value.valueOf(), undefined as never)
    : (undefined as never);
}
IsRegExp;
IsBoolean;
FromValue;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.is_empty(),
        "expected `globalThis.RegExp`/`globalThis.Boolean` to resolve cleanly, got: {codes:?}"
    );
}

#[test]
fn global_this_type_member_as_annotation_is_clean() {
    let source = r#"
let r: globalThis.RegExp = /x/;
let b: globalThis.Boolean = Boolean(1);
r;
b;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.is_empty(),
        "expected bare `globalThis.X` annotations to resolve cleanly, got: {codes:?}"
    );
}

#[test]
fn global_this_generic_type_member_accepts_type_arguments() {
    // The type-argument path must also resolve through `globalThis`.
    let source = r#"
let xs: globalThis.Array<number> = [1, 2, 3];
xs;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.is_empty(),
        "expected `globalThis.Array<number>` to resolve cleanly, got: {codes:?}"
    );
}

#[test]
fn global_this_generic_type_member_validates_argument_arity() {
    // Resolving through `globalThis` must not bypass type-argument validation:
    // `Array` takes exactly one type argument, so two is TS2314.
    let source = r#"
let xs: globalThis.Array<number, string> = [];
xs;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2314),
        "expected TS2314 for wrong type-argument count, got: {codes:?}"
    );
}

#[test]
fn global_this_unknown_type_member_reports_ts2694() {
    // Negative control: `globalThis.NotAType` is not a global type.
    let source = r#"
let a: globalThis.NotAType = 1;
a;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.contains(&2694),
        "expected TS2694 for a missing `globalThis` member, got: {codes:?}"
    );
}

#[test]
fn renamed_local_does_not_block_global_this_member() {
    // A local binding with a *different* name must not interfere with
    // `globalThis.RegExp` resolution (anti-shadowing: the structural rule is
    // about the `globalThis` anchor, not about any same-typed local).
    let source = r#"
const localPattern = /x/;
function IsRegExp(value: unknown): value is globalThis.RegExp {
  return value instanceof RegExp;
}
IsRegExp;
localPattern;
export {};
"#;
    let codes = diag_codes(source);
    assert!(
        codes.is_empty(),
        "expected a renamed local to leave `globalThis.RegExp` resolvable, got: {codes:?}"
    );
}
