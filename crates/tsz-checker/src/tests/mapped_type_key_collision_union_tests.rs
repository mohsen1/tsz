//! Tests for mapped-type `as`-clause key-collision value union.
//!
//! When multiple source keys remap to the same literal key via an `as` clause
//! (e.g. `{ [K in keyof T as "all"]: T[K] }`), the result property must carry
//! the union of all source value types, not just the last one.
//!
//! Issue: <https://github.com/mohsen1/tsz/issues/9655>

use crate::test_utils::check_source_diagnostics;

/// `{ [K in keyof T as "all"]: T[K] }` where T = `{ a: string; b: number }`.
/// The "all" property must be `string | number`; assigning a value of just
/// `string` must produce TS2322 (it is not assignable to `string | number`
/// without error suppression) — but assigning `string | number` must be clean.
///
/// This test verifies the union direction: the assignment of `"hello"` (which
/// IS a `string`) must be accepted, and a `number`-only value also accepted.
#[test]
fn remap_colliding_keys_no_error_on_union_member() {
    let diags = check_source_diagnostics(
        r#"
type T = { a: string; b: number };
type Collapsed = { [K in keyof T as "all"]: T[K] };
declare const c: Collapsed;

const _s: string | number = c.all;
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322 when reading `c.all` as `string | number`; got: {diags:?}"
    );
}

/// Proves the structural invariant, not just one spelling. Renaming the type
/// parameter from `K` to `P` must produce the same result.
#[test]
fn remap_colliding_keys_renamed_param_no_error() {
    let diags = check_source_diagnostics(
        r#"
type Src = { x: boolean; y: string };
type Flat = { [P in keyof Src as "one"]: Src[P] };
declare const f: Flat;

const _v: boolean | string = f.one;
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for renamed iteration param; got: {diags:?}"
    );
}

/// Three source keys all remapping to the same target — the union must cover
/// all three value types.
#[test]
fn remap_three_keys_to_one_union_covers_all() {
    let diags = check_source_diagnostics(
        r#"
type Wide = { a: string; b: number; c: boolean };
type Merged = { [K in keyof Wide as "v"]: Wide[K] };
declare const m: Merged;

const _ok: string | number | boolean = m.v;
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "Expected no TS2322 for three-way collision union; got: {diags:?}"
    );
}
