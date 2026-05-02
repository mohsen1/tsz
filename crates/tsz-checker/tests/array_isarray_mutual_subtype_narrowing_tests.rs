//! Tests for `Array.isArray` narrowing on union members that are
//! mutual-subtypes with `any[]` (e.g. `{ [k: string]: any }` /
//! `Record<string, any>`).
//!
//! Rule under test:
//!
//! > When `Array.isArray(x)` narrows a *union* source whose member is
//! > structurally a mutual-subtype of `any[]` (its string index signature
//! > value type is `any`, so `any[]`'s any-typed contents satisfy the
//! > index sig), the member is replaced with the predicate type `any[]` —
//! > mirroring tsc's `mapType(matching, t => isRelated(c, t) ? c : ...)`
//! > in `getNarrowedTypeWorker`.
//!
//! These tests pin the structural rule (not a single test's spelling) by
//! exercising it under multiple identifier and value-type spellings.
//!
//! Tests use the inline `{ [k: string]: any }` form rather than
//! `Record<string, any>` because `test_utils::check_source` runs with
//! `set_lib_contexts(Vec::new())` (no lib), so the `Record` alias is
//! unresolvable in this harness. The conformance corpus exercises the
//! `Record<string, any>` spelling end-to-end (see
//! `narrowingMutualSubtypes.ts`).

use crate::context::CheckerOptions;
use crate::test_utils::check_with_options;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

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

/// Different identifier spelling (`P` for the iteration variable name in
/// the for-in loop, `K` instead of `x` in the index sig) — the structural
/// rule must hold regardless of user-chosen identifier names.
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
