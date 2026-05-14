//! Tests for `import("./module").Type` used in conditional-type extends clauses.
//!
//! Structural rule: when evaluating `T extends import("./m").Type ? A : B`,
//! the `import()` type must be resolved to its structural shape before the
//! subtype check, producing the same result as `T extends ResolvedType ? A : B`.
//!
//! This covers the bug reported in issue #6801.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn no_errors(files: &[(&str, &str)], entry: &str) {
    let diags = check_multi_file(files, entry, CheckerOptions::default());
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:#?}",);
}

fn ts2322_count(files: &[(&str, &str)], entry: &str) -> usize {
    check_multi_file(files, entry, CheckerOptions::default())
        .into_iter()
        .filter(|d| d.code == 2322)
        .count()
}

// ---------------------------------------------------------------------------
// Basic case: single-segment import type in conditional extends
// ---------------------------------------------------------------------------

/// `T extends import("./types").OnlyType ? true : false` should evaluate to
/// `true` when `T = { a: string }` and `OnlyType = { a: string }`.
/// This is the exact repro from issue #6801.
#[test]
fn import_type_in_conditional_extends_single_segment() {
    let types_file = r#"export type OnlyType = { a: string };"#;
    let entry_file = r#"
type IsOnlyType<T> = T extends import("./types").OnlyType ? true : false;

type Test1 = IsOnlyType<{ a: string }>;
declare const t1: Test1;
const _check1: true = t1;  // Must NOT produce TS2322
"#;
    no_errors(
        &[("types.ts", types_file), ("entry.ts", entry_file)],
        "entry.ts",
    );
}

/// The fix must not depend on the type-parameter letter used.
/// Renaming `T` to `X` must produce the same result.
#[test]
fn import_type_in_conditional_extends_renamed_type_param() {
    let types_file = r#"export type Shape = { value: number };"#;
    let entry_file = r#"
type IsShape<X> = X extends import("./types").Shape ? true : false;
type IsShapeAlt<K> = K extends import("./types").Shape ? true : false;

type R1 = IsShape<{ value: number }>;
declare const r1: R1;
const _c1: true = r1;

type R2 = IsShapeAlt<{ value: number }>;
declare const r2: R2;
const _c2: true = r2;
"#;
    no_errors(
        &[("types.ts", types_file), ("entry.ts", entry_file)],
        "entry.ts",
    );
}

/// A non-matching type should still yield `false`.
#[test]
fn import_type_in_conditional_extends_false_branch() {
    let types_file = r#"export type OnlyType = { a: string };"#;
    let entry_file = r#"
type IsOnlyType<T> = T extends import("./types").OnlyType ? true : false;

type Test2 = IsOnlyType<{ b: number }>;
declare const t2: Test2;
const _check2: false = t2;  // Must NOT produce TS2322
"#;
    no_errors(
        &[("types.ts", types_file), ("entry.ts", entry_file)],
        "entry.ts",
    );
}

/// Assigning to the wrong branch must still produce TS2322.
#[test]
fn import_type_in_conditional_extends_wrong_branch_errors() {
    let types_file = r#"export type OnlyType = { a: string };"#;
    let entry_file = r#"
type IsOnlyType<T> = T extends import("./types").OnlyType ? true : false;

// { a: string } extends OnlyType → true, so assigning to `false` is an error
type Test = IsOnlyType<{ a: string }>;
declare const t: Test;
const _bad: false = t;  // Must produce TS2322
"#;
    assert_eq!(
        ts2322_count(
            &[("types.ts", types_file), ("entry.ts", entry_file)],
            "entry.ts"
        ),
        1,
        "expected exactly one TS2322 for assigning `true` to `false`",
    );
}

// ---------------------------------------------------------------------------
// Inline (same-file) import type — should not regress
// ---------------------------------------------------------------------------

/// Same-file type references in conditional extends must continue to work.
#[test]
fn inline_type_in_conditional_extends_same_file() {
    let source = r#"
type Shape = { x: number };
type IsShape<T> = T extends Shape ? true : false;
type R = IsShape<{ x: number }>;
declare const r: R;
const _c: true = r;
"#;
    let diags = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "same-file conditional must have no errors, got: {diags:#?}"
    );
}

// ---------------------------------------------------------------------------
// Multi-segment path: import("./m").Ns.Type
// ---------------------------------------------------------------------------

/// `import("./m").Ns.Inner` where `Ns` is a namespace with an exported type.
#[test]
fn import_type_in_conditional_extends_two_segments() {
    let types_file = r#"
export namespace Ns {
    export type Inner = { key: string };
}
"#;
    let entry_file = r#"
type IsInner<T> = T extends import("./types").Ns.Inner ? true : false;

type R = IsInner<{ key: string }>;
declare const r: R;
const _c: true = r;
"#;
    no_errors(
        &[("types.ts", types_file), ("entry.ts", entry_file)],
        "entry.ts",
    );
}
