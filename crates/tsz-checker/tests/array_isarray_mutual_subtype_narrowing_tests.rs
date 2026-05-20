//! Tests for `Array.isArray` narrowing on union members that are
//! compatible with `any[]` — both via string-index and numeric-index paths.
//!
//! Rule under test (string-index path):
//!
//! > When `Array.isArray(x)` narrows a *union* source whose member is
//! > structurally a mutual-subtype of `any[]` (its string index signature
//! > value type is `any`, so `any[]`'s any-typed contents satisfy the
//! > index sig), the member is replaced with the predicate type `any[]` —
//! > mirroring tsc's `mapType(matching, t => isRelated(c, t) ? c : ...)`
//! > in `getNarrowedTypeWorker`.
//!
//! Rule under test (numeric-index path — issue #8782):
//!
//! > When `Array.isArray(x)` narrows a union and a member `t` has a
//! > numeric index signature (`[n: number]: T`), `any[]` is structurally
//! > compatible with `t` (since `any` is assignable to every `T`), so `t`
//! > is substituted with `any[]` in the true branch. This covers
//! > `ArrayLike<T>`-style interfaces that are not literally `Array<T>` or
//! > `ReadonlyArray<T>` but are recognized as array-like via their numeric
//! > index signature.
//!
//! Both groups of tests pin the *structural* rule, not a single test's
//! spelling, by exercising at least three distinct identifier/type
//! choices per rule.
//!
//! Tests that need the built-in lib types use `load_lib_files` explicitly.
//! Tests without lib inline their own interface definitions.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{
    check_source_strict_messages as check_strict, check_source_with_libs_code_messages,
    load_lib_files,
};

fn check_strict_es5(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs_code_messages(source, "test.ts", CheckerOptions::default(), &libs)
}

/// Shared numeric-index interface used by tests that prove the rule is
/// structural and not tied to a specific element-type spelling.
const ARRAY_LIKE_IFACE: &str =
    "interface ArrayLike<T> { readonly length: number; readonly [n: number]: T; }";

// ─── String-index (Record<string, any>) path ────────────────────────────────

/// `Array.isArray(R | R[]) && obj.length` else-branch indexing emits
/// TS7053 referencing `any[] | { [x: string]: any; }` — the tsc-faithful
/// narrowed display. Without the fix the message references the
/// unnarrowed `{ [x: string]: any; } | { [x: string]: any; }[]`.
#[test]
fn array_isarray_else_branch_indexing_uses_any_array_in_message_for_record_any_union() {
    let source = r#"
function f(obj: { [x: string]: any } | { [x: string]: any }[]) {
    if (Array.isArray(obj) && obj.length) {}
    else {
        for (let k in obj) {
            obj[k];
        }
    }
}
"#;
    let diags = check_strict(source);
    let ts7053: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 7053).collect();
    assert!(
        !ts7053.is_empty(),
        "expected at least one TS7053, got: {diags:?}"
    );
    for (_, msg) in &ts7053 {
        // After the fix the narrowed source is rendered with `any[]` (the
        // predicate type) plus the non-array member — the array-shaped
        // member has been absorbed by `any[]`.
        assert!(
            msg.contains("any[]"),
            "expected TS7053 to reference `any[]` (the predicate substitution), got: {msg}"
        );
        assert!(
            !msg.contains("any; }[]"),
            "expected `{{ [x: string]: any; }}[]` to be absorbed by `any[]` after \
             mutual-subtype narrowing, got: {msg}"
        );
    }
}

#[test]
fn logical_and_false_branch_preserves_alias_record_and_array_members() {
    let source = r#"
type Record<K extends string, T> = { [P in K]: T };
function f(obj: Record<string, any> | Record<string, any>[]) {
    if (Array.isArray(obj) && obj.length) {}
    else {
        for (let key in obj) {
            obj[key];
        }
    }
}
"#;
    let diags = check_strict(source);
    let ts7053_messages: Vec<&String> = diags
        .iter()
        .filter_map(|(c, m)| (*c == 7053).then_some(m))
        .collect();
    assert!(
        !ts7053_messages.is_empty(),
        "expected TS7053, got: {diags:?}"
    );
    for msg in &ts7053_messages {
        assert!(
            msg.contains("any[]"),
            "logical `&&` false-branch joins must preserve the array branch, got: {msg}"
        );
        assert!(
            msg.contains("Record<string, any>"),
            "logical `&&` false-branch joins must also preserve the alias record branch, got: {msg}"
        );
    }
}

/// Different identifier spelling — the structural rule must hold regardless
/// of user-chosen identifier names.
#[test]
fn array_isarray_narrowing_independent_of_user_chosen_names() {
    let source = r#"
function f(obj: { [K: string]: any } | { [K: string]: any }[]) {
    if (Array.isArray(obj) && obj.length) {}
    else {
        for (let P in obj) {
            obj[P];
        }
    }
}
"#;
    let diags = check_strict(source);
    let ts7053_messages: Vec<&String> = diags
        .iter()
        .filter_map(|(c, m)| (*c == 7053).then_some(m))
        .collect();
    assert!(
        !ts7053_messages.is_empty(),
        "expected TS7053, got: {diags:?}"
    );
    for msg in &ts7053_messages {
        assert!(
            msg.contains("any[]"),
            "expected TS7053 to reference `any[]`, got: {msg}"
        );
    }
}

/// Pure `Array.isArray(R | R[])` true branch narrows to `any[]` (collapsed),
/// not `R[]`. Verified by passing the narrowed value to a `T extends never`
/// parameter and inspecting the TS2345 message — which renders the actual
/// narrowed type.
#[test]
fn array_isarray_true_branch_collapses_record_any_union_to_any_array() {
    let source = r#"
type R = { [x: string]: any };
declare function expect_never<T extends never>(x: T): void;
function f(obj: R | R[]) {
    if (Array.isArray(obj)) {
        expect_never(obj);
    }
}
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {ts2345:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("any[]"),
        "expected TS2345 to render the narrowed source as `any[]`, got: {msg}"
    );
    assert!(
        !msg.contains("R[]") && !msg.contains("any; }[]"),
        "expected the array-shaped member to be absorbed by `any[]` after \
         mutual-subtype narrowing, got: {msg}"
    );
}

/// `Array.isArray` false branch on `R | R[]` keeps the non-array part `R`.
/// This is the existing behavior and should be preserved by the fix.
#[test]
fn array_isarray_false_branch_keeps_record_any_part() {
    let source = r#"
type R = { [x: string]: any };
declare function expect_never<T extends never>(x: T): void;
function f(obj: R | R[]) {
    if (!Array.isArray(obj)) {
        expect_never(obj);
    }
}
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {ts2345:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        !msg.contains("any; }[]") && !msg.contains("R[]"),
        "expected the array part to be excluded by `!Array.isArray`, got: {msg}"
    );
}

/// `Array.isArray` narrowing must NOT replace `string[]` (whose element
/// `string` is not any-compat) with `any[]` — the existing array-narrowing
/// path must keep concrete element types.
#[test]
fn array_isarray_preserves_concrete_array_element_types() {
    let source = r#"
declare function expect_never<T extends never>(x: T): void;
function f(obj: string[] | number) {
    if (Array.isArray(obj)) {
        expect_never(obj);
    }
}
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expected exactly one TS2345, got: {ts2345:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("string[]"),
        "expected TS2345 to render the narrowed source as `string[]`, got: {msg}"
    );
    assert!(
        !msg.contains("any[]"),
        "expected concrete `string[]` to be preserved, not collapsed to `any[]`: {msg}"
    );
}

#[test]
fn array_isarray_preserves_readonly_generic_array_union_member() {
    let source = r#"
interface ReadonlyArray<T> {
  readonly [n: number]: T;
}
declare function expect_never<T extends never>(x: T): void;
interface TestCase<T extends string | number> {
  readonly val1: T | ReadonlyArray<T>;
}

declare const item: TestCase<string | number>;

if (Array.isArray(item.val1)) {
  expect_never(item.val1);
}
"#;
    let diags = check_strict(source);
    let ts2345: Vec<&(u32, String)> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Array.isArray should preserve a non-never readonly array branch, got: {diags:?}"
    );
}

// ─── Numeric-index path (issue #8782) ───────────────────────────────────────

/// `ArrayLike<T>` (which has a numeric index `[n: number]: T`) should be
/// kept (substituted as `any[]`) in the `Array.isArray` true branch, not
/// dropped to `never`. Three different element types to confirm the rule is
/// structural, not keyed on element type spelling.
#[test]
fn array_isarray_numeric_index_interface_substituted_with_any_array_number_element() {
    let source = format!(
        "{ARRAY_LIKE_IFACE}\n{body}",
        body = r#"
declare function expect_never<T extends never>(x: T): void;
function f(x: ArrayLike<number> | string) {
    if (Array.isArray(x)) {
        expect_never(x);
    }
}
"#
    );
    let diags = check_strict(&source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "ArrayLike<number>|string true branch should not produce TS2339: {ts2339:?}"
    );
    // The narrowed type must be non-never (expect_never must fail).
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expect_never in true branch should produce exactly one TS2345 (narrowed != never): {diags:?}"
    );
}

#[test]
fn array_isarray_numeric_index_interface_substituted_with_any_array_string_element() {
    // Same rule, different element type spelling (string instead of number).
    let source = r#"
interface ListOf<T> {
    readonly length: number;
    readonly [i: number]: T;
}
declare function expect_never<T extends never>(x: T): void;
function f(x: ListOf<string> | boolean) {
    if (Array.isArray(x)) {
        expect_never(x);
    }
}
"#;
    let diags = check_strict(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "ListOf<string>|boolean true branch should not produce TS2339: {ts2339:?}"
    );
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expect_never in true branch should produce exactly one TS2345: {diags:?}"
    );
}

#[test]
fn array_isarray_numeric_index_interface_substituted_with_any_array_generic_element() {
    // Same rule, generic element type (T as type param).
    let source = r#"
interface Seq<T> {
    readonly length: number;
    readonly [k: number]: T;
}
declare function expect_never<T extends never>(x: T): void;
function f<T>(x: Seq<T> | T) {
    if (Array.isArray(x)) {
        expect_never(x);
    }
}
"#;
    let diags = check_strict(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Seq<T>|T true branch should not produce TS2339: {ts2339:?}"
    );
    // The narrowed type must be non-never (expect_never must fail).
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "expect_never in true branch should produce exactly one TS2345 (narrowed != never): {diags:?}"
    );
}

/// False branch for numeric-indexed types: the type is KEPT (not excluded),
/// because `ArrayLike<T>` objects can fail `Array.isArray` at runtime.
#[test]
fn array_isarray_false_branch_keeps_numeric_index_interface() {
    let source = format!(
        "{ARRAY_LIKE_IFACE}\n{body}",
        body = r#"
function f(x: ArrayLike<number> | string) {
    if (!Array.isArray(x)) {
        x;
    }
}
"#
    );
    let diags = check_strict(&source);
    // No errors expected: both ArrayLike<number> and string are valid in false branch.
    let errors: Vec<_> = diags
        .iter()
        .filter(|(c, _)| matches!(*c, 2339 | 2322 | 2345))
        .collect();
    assert!(
        errors.is_empty(),
        "false branch should keep ArrayLike<T> and string, got: {errors:?}"
    );
}

/// `x.length` is accessible in the true branch for a numeric-indexed type.
#[test]
fn array_isarray_true_branch_length_access_on_numeric_index_type() {
    let source = format!(
        "{ARRAY_LIKE_IFACE}\n{body}",
        body = r#"
function f(x: ArrayLike<boolean> | null) {
    if (Array.isArray(x)) {
        x.length;
    }
}
"#
    );
    let diags = check_strict(&source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "x.length after Array.isArray on ArrayLike<boolean>|null: {ts2339:?}"
    );
}

/// `ReadonlyArray<T>` (lib-style) is kept in the true branch. Confirms the
/// existing Application-type path still works alongside the new numeric-index
/// path.
#[test]
fn array_isarray_readonly_array_with_lib_kept_in_true_branch() {
    let source = r#"
function f(x: ReadonlyArray<number> | number) {
    if (Array.isArray(x)) {
        x.length;
    }
}
"#;
    let diags = check_strict_es5(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "ReadonlyArray<number>|number should narrow x.length: {ts2339:?}"
    );
}

/// `ArrayLike<T>` from the lib (alongside the built-in `Array`) is handled
/// correctly when lib is loaded.
#[test]
fn array_isarray_arraylike_with_lib_kept_in_true_branch() {
    let source = r#"
interface MyArrayLike<T> {
    readonly length: number;
    readonly [n: number]: T;
}
function f(x: MyArrayLike<number> | string) {
    if (Array.isArray(x)) {
        x.length;
    }
}
"#;
    let diags = check_strict_es5(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "MyArrayLike<number>|string with lib should narrow x.length: {ts2339:?}"
    );
}

/// `readonly T[]` syntax form is kept in the true branch — verifying the
/// `ReadonlyType(Array(T))` path is still handled by `is_array_like`.
#[test]
fn array_isarray_readonly_array_syntax_kept_in_true_branch() {
    let source = r#"
function f(x: readonly number[] | string) {
    if (Array.isArray(x)) {
        x.length;
    }
}
"#;
    let diags = check_strict(source);
    let ts2339: Vec<_> = diags.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "readonly number[]|string should narrow x.length: {ts2339:?}"
    );
}

/// Pure primitives (`string`, `number`, `boolean`) have no numeric index
/// signature, so they must NOT be substituted in the true branch (they are
/// excluded → never).
#[test]
fn array_isarray_excludes_primitive_without_numeric_index() {
    let source = r#"
function f(x: string | number) {
    if (Array.isArray(x)) {
        x;
    }
}
"#;
    let diags = check_strict(source);
    // TS2304 "Cannot find name 'Array'" is expected in the no-lib harness.
    let unexpected: Vec<_> = diags.iter().filter(|(c, _)| *c != 2304).collect();
    assert!(
        unexpected.is_empty(),
        "string|number true branch should compile cleanly, got: {unexpected:?}"
    );
}
