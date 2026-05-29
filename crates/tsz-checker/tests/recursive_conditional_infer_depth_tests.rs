//! Recursive conditional aliases with non-progressing `infer` branches.
//!
//! Structural rule: when a recursive conditional alias recurses on the same
//! input after an `infer` branch, `tsc` reports bounded TS2589 instead of
//! overflowing. The checker must surface the same depth diagnostic and recover.

use tsz_checker::test_utils::check_source_codes;

#[track_caller]
fn assert_codes(source: &str, expected: &[u32], label: &str) {
    let actual = check_source_codes(source);
    assert_eq!(
        actual, expected,
        "[{label}] expected diagnostic codes {expected:?}, got {actual:?}"
    );
}

#[test]
fn recursive_infer_branch_on_same_input_emits_ts2589() {
    let source = r#"
type Recur<T> = T extends { a: infer U } ? U | Recur<T> : never;
type Actual = Recur<{ a: { b: string } | { c: number } }>;
declare const actual: Actual;
const useActual: { b: string } | { c: number } = actual;
const bad: { d: boolean } = actual;
"#;

    assert_codes(source, &[2589], "same-input recursive infer branch");
}

#[test]
fn renamed_recursive_infer_branch_on_same_input_emits_ts2589() {
    let source = r#"
type PickAgain<Item> = Item extends { value: infer Part } ? Part | PickAgain<Item> : never;
type Result = PickAgain<{ value: { left: string } | { right: number } }>;
declare const result: Result;
const useResult: { left: string } | { right: number } = result;
const bad: { done: boolean } = result;
"#;

    assert_codes(source, &[2589], "renamed same-input recursive infer branch");
}

#[test]
fn terminating_recursive_infer_branch_does_not_emit_ts2589() {
    let source = r#"
type Unbox<T> = T extends { a: infer U } ? U : never;
type Actual = Unbox<{ a: { b: string } | { c: number } }>;
declare const actual: Actual;
const ok: { b: string } | { c: number } = actual;
"#;

    assert_codes(source, &[], "terminating infer branch");
}

#[test]
fn non_matching_same_input_recursive_condition_does_not_emit_ts2589() {
    let source = r#"
type Recur<T> = T extends { a: infer U } ? U | Recur<T> : never;
type Actual = Recur<string>;
declare const actual: Actual;
const ok: never = actual;
"#;

    assert_codes(source, &[], "non-matching same-input recursive condition");
}

#[test]
fn productive_recursive_object_branch_does_not_emit_ts2589() {
    let source = r#"
type Node<T> = T extends unknown ? { value: T; next: Node<T> } : never;
type Actual = Node<number>;
declare const actual: Actual;
const ok: number = actual.next.next.value;
"#;

    assert_codes(source, &[], "productive recursive object branch");
}
