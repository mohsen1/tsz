//! Tests for generic self-circular type aliases (issue #9781).
//!
//! A generic type alias whose unwrapped body is a self-application
//! (`type A<T> = A<T>`, or a cycle through simple-reference alias hops)
//! collapses to a non-generic error type in tsc: TS2456 at the declaration
//! plus TS2315 ("Type 'X' is not generic") at the body self-reference and at
//! every site that applies type arguments to it. Before this fix tsz silently
//! accepted the generic form (exit 0, no diagnostics) while the non-generic
//! form (`type A = A`) was already detected.
//!
//! Structural rule: when a generic alias's unwrapped body is a simple type
//! reference that cycles back to the alias through simple-reference hops, the
//! alias is circular and non-generic. Structural wrappers (object/array/
//! union/intersection/function/conditional/...) defer resolution and break the
//! cycle, so legitimate recursion such as `type List<T> = T | List<T>[]` is
//! never flagged.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn reported_repro_generic_self_application() {
    // `type A<T> = A<T>; const x: A<number> = 1;`
    let source = "type A<T> = A<T>;\nconst x: A<number> = 1;\n";
    let c = codes(source);
    assert!(
        c.contains(&2456),
        "expected TS2456 at the circular declaration. got: {c:?}"
    );
    assert!(
        c.iter().filter(|&&x| x == 2315).count() >= 2,
        "expected TS2315 at the body self-ref and the use site. got: {c:?}"
    );
    assert!(
        !c.contains(&2322),
        "the collapsed alias is an error type; no cascading TS2322. got: {c:?}"
    );
}

/// Anti-hardcoding: a renamed alias with a separate use site behaves
/// identically. The rule must key on the structural cycle, not the name `A`.
#[test]
fn renamed_alias_with_separate_use_site() {
    let source = "type Foo<T> = Foo<T>;\ntype Bar<T> = Foo<T>;\nconst x: Bar<number> = 1;\n";
    let c = codes(source);
    assert!(
        c.contains(&2456),
        "expected TS2456 for the circular Foo alias. got: {c:?}"
    );
    assert!(
        c.contains(&2315),
        "expected TS2315 at use sites. got: {c:?}"
    );
}

/// A mutual two-alias cycle through simple references is circular for both.
#[test]
fn mutual_generic_alias_cycle() {
    let source = "type Foo<T> = Bar<T>;\ntype Bar<T> = Foo<T>;\nconst x: Foo<number> = 1;\n";
    let c = codes(source);
    assert!(
        c.iter().filter(|&&x| x == 2456).count() >= 1,
        "expected TS2456 on the mutual cycle. got: {c:?}"
    );
    assert!(
        c.contains(&2315),
        "expected TS2315 at use sites. got: {c:?}"
    );
}

/// A three-hop cycle proves the detection follows simple-reference hops, not
/// just a single direct self-application.
#[test]
fn three_hop_generic_alias_cycle() {
    let source =
        "type A<T> = B<T>;\ntype B<T> = C<T>;\ntype C<T> = A<T>;\nconst x: A<number> = 1;\n";
    let c = codes(source);
    assert!(
        c.iter().filter(|&&x| x == 2456).count() >= 1,
        "expected TS2456 on the three-hop cycle. got: {c:?}"
    );
}

/// An unused (declaration-only) generic circular alias still reports TS2456 at
/// the declaration and TS2315 at the body self-reference.
#[test]
fn unused_generic_circular_alias_still_errors() {
    let source = "type Baz<T> = Baz<T>;\n";
    let c = codes(source);
    assert!(
        c.contains(&2456),
        "expected TS2456 on the unused circular alias. got: {c:?}"
    );
    assert!(
        c.contains(&2315),
        "expected TS2315 at the body self-reference. got: {c:?}"
    );
}

/// A parenthesized body is unwrapped before the cycle check; same result.
#[test]
fn parenthesized_generic_circular_alias() {
    let source = "type A<T> = (A<T>);\nconst x: A<number> = 1;\n";
    let c = codes(source);
    assert!(c.contains(&2456), "expected TS2456. got: {c:?}");
    assert!(c.contains(&2315), "expected TS2315. got: {c:?}");
}

/// Multiple type parameters do not change the rule.
#[test]
fn multi_param_generic_circular_alias() {
    let source = "type A<T, U> = A<T, U>;\nconst x: A<number, string> = 1;\n";
    let c = codes(source);
    assert!(c.contains(&2456), "expected TS2456. got: {c:?}");
    assert!(c.contains(&2315), "expected TS2315. got: {c:?}");
}

/// Regression guard: the non-generic circular alias was already detected and
/// must keep emitting TS2456.
#[test]
fn non_generic_circular_alias_still_emits_ts2456() {
    let source = "type A = A;\nconst x: A = 1;\n";
    let c = codes(source);
    assert!(
        c.contains(&2456),
        "non-generic circular alias must still emit TS2456. got: {c:?}"
    );
}

/// Negative: a legitimate recursive generic alias whose self-reference is
/// structurally wrapped (union + array) must NOT error.
#[test]
fn legitimate_recursive_alias_not_flagged() {
    let source = "type List<T> = T | List<T>[];\nconst x: List<number> = [1, [2], 3];\n";
    let c = codes(source);
    assert!(
        !c.contains(&2456) && !c.contains(&2315),
        "legitimate recursive alias must not error. got: {c:?}"
    );
}

/// Negative: a recursive conditional alias (a common, valid pattern) must NOT
/// be treated as circular.
#[test]
fn legitimate_recursive_conditional_alias_not_flagged() {
    let source = "type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;\nconst x: Flatten<number[][]> = 1;\n";
    let c = codes(source);
    assert!(
        !c.contains(&2456),
        "recursive conditional alias must not be circular. got: {c:?}"
    );
}

/// Negative: an identity alias whose body is its own type parameter is not
/// circular (the body resolves to the parameter, not the alias).
#[test]
fn identity_alias_not_flagged() {
    let source = "type Identity<T> = T;\nconst x: Identity<number> = 1;\n";
    let c = codes(source);
    assert!(
        !c.contains(&2456) && !c.contains(&2315),
        "identity alias must not error. got: {c:?}"
    );
}
