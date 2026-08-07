//! Regression coverage for issue #16707: a function-shaped union member must
//! survive union *narrowing* (redundant-member removal) against a weak
//! (all-optional) sibling member — not just direct assignability — when the
//! union comes from generic instantiation (a type parameter's inferred
//! signature, or a mapped-type property) rather than a literal annotation.
//!
//! Rule: `tsc`'s own union-narrowing pass (`removeSubtypes`, used while
//! building the type of an instantiated union) runs `strictSubtypeRelation`,
//! which still performs the weak-type "no common properties" veto (TS2559)
//! even though it isn't `ComparableRelation`. tsz's evaluator-level
//! `remove_redundant_members` used a plain structural check with no such
//! veto, so a callable member that shares no properties with an all-optional
//! sibling (e.g. `(() => T) | { get?(): T }`) was wrongly treated as
//! subsumed by it and dropped from the union — collapsing
//! `(() => number) | Computed<number>` down to just `Computed<number>` and
//! reporting TS2559/TS2322 against an argument that only matches the
//! function member.
//!
//! The behavior keys on type structure (callable vs weak-object sibling,
//! whether the union arrives via instantiation), never on
//! identifier/property/type-parameter names, so these tests vary all of
//! those.

use tsz_checker::test_utils::check_source_code_messages;

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

#[test]
fn function_arg_against_inferred_union_with_weak_sibling_is_accepted() {
    // The exact repro from #16707: T is inferred from the `() => T` member,
    // but the union-narrowing pass must not drop that member first.
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
declare function g1<T>(x: (() => T) | Computed<T>): T;
let b1 = g1(() => 1);
"#,
    );
    assert!(
        diags.is_empty(),
        "a callable argument must match the callable union member, not be rejected via a dropped member: {diags:?}"
    );
}

#[test]
fn function_arg_against_inferred_union_with_weak_sibling_first_is_accepted() {
    // Member order must not matter — the weak member listed first.
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
declare function g1<T>(x: Computed<T> | (() => T)): T;
let b1 = g1(() => 1);
"#,
    );
    assert!(diags.is_empty(), "member order must not matter: {diags:?}");
}

#[test]
fn function_arg_against_mapped_type_union_with_weak_sibling_is_accepted() {
    // A mapped-type property union, concrete instantiation (issue's case C).
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
type Acc<T> = { [K in keyof T]: (() => T[K]) | Computed<T[K]> };
declare function takeC(a: Acc<{ test: number }>): void;
takeC({ test(): number { return 1; } });
"#,
    );
    assert!(
        diags.is_empty(),
        "mapped-type instantiation must not drop the callable member: {diags:?}"
    );
}

#[test]
fn function_arg_against_mapped_type_union_inferred_is_accepted() {
    // Mapped-type property union, inferred (issue's case D).
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
type Acc<T> = { [K in keyof T]: (() => T[K]) | Computed<T[K]> };
declare function takeD<P>(a: Acc<P>): P;
takeD({ test(): number { return 1; } });
"#,
    );
    assert!(
        diags.is_empty(),
        "an inferred mapped-type union must not drop the callable member: {diags:?}"
    );
}

#[test]
fn function_arg_against_directly_annotated_union_stays_accepted() {
    // Regression guard: the directly annotated (non-generic) union was
    // already correct before the fix — must stay correct after it.
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
let a1: (() => number) | Computed<number> = () => 1;
let a2: Computed<number> | (() => number) = () => 1;
"#,
    );
    assert!(
        diags.is_empty(),
        "a direct annotation must stay clean: {diags:?}"
    );
}

#[test]
fn renamed_binders_function_arg_against_inferred_union_is_accepted() {
    // Anti-hardcoding: identical shape, different type/function/parameter names.
    let diags = check_source_code_messages(
        r#"
type Deferred<Value> = { read?(): Value; write?(next: Value): void; };
declare function resolveEntry<Item>(getter: (() => Item) | Deferred<Item>): Item;
let entry = resolveEntry(() => "hi");
"#,
    );
    assert!(
        diags.is_empty(),
        "renamed binders must behave identically: {diags:?}"
    );
}

#[test]
fn two_weak_members_plus_callable_inferred_is_accepted() {
    // More than one weak sibling in the union.
    let diags = check_source_code_messages(
        r#"
type W<T> = { get?(): T } | { set?(v: T): void } | (() => T);
declare function g3<T>(x: W<T>): T;
let f1 = g3(() => 1);
"#,
    );
    assert!(
        diags.is_empty(),
        "multiple weak siblings must not change the result: {diags:?}"
    );
}

#[test]
fn object_arg_with_no_common_property_against_inferred_union_still_errors() {
    // Negative control: an object literal that matches NEITHER the callable
    // member NOR any property of the weak member must still be rejected —
    // the fix must not disable the weak-type check outright, only stop it
    // from wrongly collapsing the union first.
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
declare function g2<T>(x: (() => T) | Computed<T>): T;
let e1 = g2({ nope: 1 });
"#,
    );
    assert!(
        has_code(&diags, 2353) || has_code(&diags, 2559) || has_code(&diags, 2345),
        "an argument matching neither union member must still be rejected: {diags:?}"
    );
}

#[test]
fn object_matching_weak_member_against_inferred_union_is_accepted() {
    // Positive control on the OTHER member: an object that genuinely
    // satisfies the weak member's declared property must still be accepted.
    let diags = check_source_code_messages(
        r#"
type Computed<T> = { get?(): T; set?(value: T): void; };
declare function g4<T>(x: (() => T) | Computed<T>): T;
let g = g4({ get() { return 1; } });
"#,
    );
    assert!(
        diags.is_empty(),
        "an object satisfying the weak member must still be accepted: {diags:?}"
    );
}
