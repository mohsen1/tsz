//! Regression coverage for `TS2677` (type-predicate type must be assignable to
//! its parameter's type) on the function-**type**-node path when the parameter
//! and/or asserted type is written through a type alias.
//!
//! Structural rule: a predicate `p is X` is valid when `X` is assignable to
//! `p`'s declared type. When either side is written through a type alias — a
//! `Lazy(DefId)` head (`type A = string`) or a generic-alias `Application`
//! (`Alias<T> = keyof T`) — tsc resolves the alias to its body and relates
//! structurally. The type-node predicate validation must therefore run the
//! relation through the checker's `DefId`-resolving environment; with a no-op
//! resolver the alias stays opaque and a sound predicate spuriously fails.
//!
//! Binder names are varied across cases so the parity is structural and not
//! pinned to any particular identifier text.

use crate::test_utils::check_source_strict_codes;

fn ts2677_count(source: &str) -> usize {
    check_source_strict_codes(source)
        .into_iter()
        .filter(|&code| code == 2677)
        .count()
}

#[test]
fn alias_typed_parameter_with_concrete_predicate_is_clean() {
    // `type Name = string; (p: Name) => p is string` — alias on the parameter
    // type only. tsc resolves `Name` to `string`; the predicate is sound.
    let src = r#"
type Name = string;
let probe: (p: Name) => p is string = (p): p is string => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn alias_typed_asserted_type_is_clean() {
    // Alias on the asserted type only: `(p: string) => p is Label`.
    let src = r#"
type Label = string;
let probe: (value: string) => value is Label = (value): value is Label => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn alias_typed_on_both_sides_is_clean() {
    // Alias on both parameter and asserted type.
    let src = r#"
type Token = string;
let probe: (entry: Token) => entry is Token = (entry): entry is Token => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn nested_alias_chain_is_clean() {
    // A chain of aliases must resolve to the underlying body.
    let src = r#"
type Base = string;
type Wrapped = Base;
let probe: (item: Wrapped) => item is string = (item): item is string => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn generic_alias_keyof_application_is_clean() {
    // `Keys<T> = keyof T`: the parameter is a deferred generic-alias
    // `Application`. tsc resolves it to `keyof T` and relates structurally.
    let src = r#"
type Keys<T> = keyof T;
let probe: <T>(key: Keys<T>) => key is keyof T = <T>(key): key is keyof T => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn alias_to_union_with_narrower_predicate_is_clean() {
    // Alias to a union; a predicate narrowing to one member is sound.
    let src = r#"
type Scalar = string | number;
let probe: (slot: Scalar) => slot is string = (slot): slot is string => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 0);
}

#[test]
fn genuine_mismatch_through_alias_still_reports_ts2677() {
    // NEGATIVE: alias-typed parameter `Handle = string`, predicate asserts
    // `number`. The alias resolves, the relation genuinely fails — TS2677 must
    // still fire, exactly as tsc.
    let src = r#"
type Handle = string;
let broken: (h: Handle) => h is number = (h): h is number => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 1);
}

#[test]
fn genuine_mismatch_without_alias_still_reports_ts2677() {
    // NEGATIVE control with no alias: `(p: string) => p is number`.
    let src = r#"
let broken: (p: string) => p is number = (p): p is number => true;
export {};
"#;
    assert_eq!(ts2677_count(src), 1);
}
