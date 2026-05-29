//! Recursive conditional evaluation with indexed-access recursion.
//!
//! Structural rule: when a distributive recursive conditional's true branch
//! recurses through an indexed access of the narrowed array member, transparent
//! aliases around that indexed access must not break the recursion frontier.
//! `R<Id<T[0]>>` should evaluate like `R<T[0]>`: one branch per array-member
//! union member, no eager TS2589 at the alias definition.

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
fn recursive_conditional_direct_indexed_access_distributes_array_union() {
    let source = r#"
type Recur<T> = T extends any[] ? { k: Recur<T[0]> } : { v: T };
type Actual = Recur<[string] | [number]>;
declare const actual: Actual;
const ok: { k: { v: string } } | { k: { v: number } } = actual;
const bad: { k: { v: boolean } } = actual;
"#;

    assert_codes(source, &[2322], "direct recursive indexed access");
}

#[test]
fn recursive_conditional_alias_wrapped_indexed_access_distributes_array_union() {
    let source = r#"
type Identity<T> = T;
type Recur<T> = T extends any[] ? { k: Recur<Identity<T[0]>> } : { v: T };
type Actual = Recur<Identity<[string] | [number]>>;
declare const actual: Actual;
const ok: { k: { v: string } } | { k: { v: number } } = actual;
const bad: { k: { v: boolean } } = actual;
"#;

    assert_codes(source, &[2322], "alias-wrapped recursive indexed access");
}

#[test]
fn recursive_conditional_renamed_alias_wrapped_indexed_access_distributes() {
    let source = r#"
type Same<Value> = Value;
type Walk<Item> = Item extends any[] ? { k: Walk<Same<Item[0]>> } : { v: Item };
type Actual = Walk<Same<[string] | [number]>>;
declare const actual: Actual;
const ok: { k: { v: string } } | { k: { v: number } } = actual;
const bad: { k: { v: boolean } } = actual;
"#;

    assert_codes(
        source,
        &[2322],
        "renamed alias-wrapped recursive indexed access",
    );
}
