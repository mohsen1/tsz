//! Regression tests for issue #12876.
//!
//! Recursive array-flattening utilities take a parameter whose element type is a
//! union of a naked type parameter and a (possibly recursive) array arm, e.g.
//! `RecArray<T> = Array<T | RecArray<T>>` or the inlined `Array<T | Array<T>>`.
//! When such a function is called with a mixed array literal, tsc's
//! `inferToMultipleTypes` routes every array-shaped constituent through the
//! structured arm and *unions* the remaining (naked) constituents into a single
//! candidate for the type parameter. tsz previously inferred each constituent
//! separately, so common-supertype resolution kept only the leftmost branch and
//! dropped the rest — collapsing `string | number` to `string` and emitting a
//! spurious `TS2322` on the well-typed mixed-element calls.
//!
//! These tests pin the corrected behaviour against several structurally distinct
//! spellings and renamed binders (the rule is structural, not tied to the
//! `RecArray`/`T` identifiers), matching the conformance fixture
//! `recursiveTypeReferences1.ts`.
//!
//! The shared test harness runs without lib definitions, so each source supplies
//! its own minimal `Array<T>` interface; this is what gives the array literals
//! and the recursive alias their array structure.

use tsz_checker::test_utils::{check_with_options_code_messages, strict_checker_options};

const ARRAY_LIB: &str = "interface Array<T> { length: number; [n: number]: T; }\n";

fn ts2322(body: &str) -> Vec<String> {
    let source = format!("{ARRAY_LIB}{body}");
    check_with_options_code_messages(&source, strict_checker_options())
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn recursive_alias_mixed_elements_infer_union_no_error() {
    // `flat([1, 'a', [2]])` / `flat([1, [2, 'a']])`: T = string | number, clean.
    let messages = ts2322(
        r#"
type RecArray<T> = Array<T | RecArray<T>>;
declare function flat<T>(a: RecArray<T>): Array<T>;
flat([1, 'a', [2]]);
flat([1, [2, 'a']]);
"#,
    );
    assert_eq!(
        messages,
        Vec::<String>::new(),
        "mixed element calls must infer T = string | number and not error"
    );
}

#[test]
fn recursive_alias_conflicting_naked_element_reports_widened_ts2322() {
    // `flat([1, ['a']])`: the string only reaches T through the recursive arm,
    // so T = string and the bare `1` (widened to `number`) is rejected.
    let messages = ts2322(
        r#"
type RecArray<T> = Array<T | RecArray<T>>;
declare function flat<T>(a: RecArray<T>): Array<T>;
flat([1, ['a']]);
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "exactly one TS2322 expected, got {messages:?}"
    );
    assert_eq!(
        messages[0],
        "Type 'number' is not assignable to type 'string | RecArray<string>'."
    );
}

#[test]
fn inlined_one_level_alias_matches_tsc_messages() {
    // `flat1<T>(a: Array<T | Array<T>>)`.
    let messages = ts2322(
        r#"
declare function flat1<T>(a: Array<T | Array<T>>): Array<T>;
flat1([1, 'a', [2]]);
flat1([1, [2, 'a']]);
flat1([1, ['a']]);
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "only the [1, ['a']] call should error, got {messages:?}"
    );
    assert_eq!(
        messages[0],
        "Type 'number' is not assignable to type 'string | string[]'."
    );
}

#[test]
fn inlined_two_level_alias_matches_tsc_messages() {
    // `flat2<T>(a: Array<T | Array<T | Array<T>>>)`.
    let messages = ts2322(
        r#"
declare function flat2<T>(a: Array<T | Array<T | Array<T>>>): Array<T>;
flat2([1, 'a', [2]]);
flat2([1, [2, 'a']]);
flat2([1, ['a']]);
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "only the [1, ['a']] call should error, got {messages:?}"
    );
    assert_eq!(
        messages[0],
        "Type 'number' is not assignable to type 'string | (string | string[])[]'."
    );
}

#[test]
fn renamed_binders_behave_identically() {
    // The fix is structural: renaming the alias, the type parameter, and the
    // parameter must not change the result.
    let messages = ts2322(
        r#"
type Nested<Elem> = Array<Elem | Nested<Elem>>;
declare function squash<Elem>(input: Nested<Elem>): Array<Elem>;
squash([10, 'x', [20]]);
squash([10, [20, 'x']]);
squash([10, ['x']]);
"#,
    );
    assert_eq!(
        messages.len(),
        1,
        "renamed binders must still error exactly once, got {messages:?}"
    );
    assert_eq!(
        messages[0],
        "Type 'number' is not assignable to type 'string | Nested<string>'."
    );
}
