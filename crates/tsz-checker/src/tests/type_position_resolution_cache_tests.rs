//! Soundness guard for the type-position identifier resolution memo (#13987).
//!
//! Recursive type evaluation re-lowers the same alias-body identifier nodes at
//! every recursion level. `resolve_identifier_symbol_in_type_position` now
//! memoizes the context-free portion of that resolution (alias / global /
//! namespace / module-augmentation) under an `(arena, node)` key, while the
//! context-sensitive enclosing-type-parameter fast path is never cached.
//!
//! These tests pin the behaviors the memo must preserve: deep recursive
//! resolution stays correct, a name used as both a type parameter and a
//! top-level alias resolves to distinct entities, the result is not keyed on
//! identifier text, and genuine mismatches are still reported.

use crate::test_utils::check_source_diagnostics;

fn diagnostic_summaries(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| format!("TS{}: {}", diagnostic.code, diagnostic.message_text))
        .collect()
}

#[test]
fn deep_recursive_type_position_resolution_is_clean() {
    // The #13987 repro shape: a recursive mapped-conditional alias applied to a
    // deeply-nested object. Every recursion level re-resolves `DeepReadonly`,
    // `T`, and `K`; the memo must not change the (clean) result.
    let diags = diagnostic_summaries(
        r#"
type DeepReadonly<T> = T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;

interface Nested {
    a: { b: { c: { d: { e: number } } } };
    f: string;
}

declare const value: DeepReadonly<Nested>;
const leaf: number = value.a.b.c.d.e;
const label: string = value.f;
"#,
    );
    assert!(
        diags.is_empty(),
        "deep recursive type-position resolution must stay clean; got {diags:?}"
    );
}

#[test]
fn type_param_and_alias_sharing_a_name_resolve_distinctly() {
    // `T` is a type parameter inside `Box<T>` (the uncached, context-sensitive
    // path) and a top-level type alias elsewhere (the cached path). Caching the
    // alias resolution must not leak into the type-parameter binding or vice
    // versa.
    let diags = diagnostic_summaries(
        r#"
type Box<T> = { value: T };
type T = string;

const boxed: Box<number> = { value: 1 };
const aliased: T = "hello";
const fromBox: number = boxed.value;
const fromAlias: string = aliased;
"#,
    );
    assert!(
        diags.is_empty(),
        "a type parameter and a same-named alias must resolve distinctly; got {diags:?}"
    );
}

#[test]
fn type_position_resolution_is_not_keyed_on_identifier_text() {
    // Renamed-binder adjacent: the same structure with different identifier
    // names must behave identically, proving the memo is keyed on (arena, node),
    // not on the spelling of the identifier.
    let diags = diagnostic_summaries(
        r#"
type Wrapper<Payload> = { readonly [Key in keyof Payload]: Wrapper<Payload[Key]> };

interface Tree {
    left: { leaf: number };
    right: { leaf: number };
}

declare const tree: Wrapper<Tree>;
const value: number = tree.left.leaf;
"#,
    );
    assert!(
        diags.is_empty(),
        "renamed type-position binders must stay clean; got {diags:?}"
    );
}

#[test]
fn cached_type_position_resolution_still_reports_real_mismatch() {
    // Negative case: the memo must not suppress a genuine error. The second
    // `Box<number>` assignment is wrong and must still report TS2322 exactly
    // once, even though `Box` is resolved (and cached) from the first use.
    let diags = diagnostic_summaries(
        r#"
type Box<T> = { value: T };

const good: Box<number> = { value: 1 };
const bad: Box<number> = { value: "no" };
"#,
    );
    assert_eq!(
        diags.len(),
        1,
        "exactly one mismatch must be reported; got {diags:?}"
    );
    assert!(
        diags[0].starts_with("TS2322:"),
        "expected TS2322 for the real mismatch; got {diags:?}"
    );
}
