//! Checker integration tests for resolving type aliases in a function-type-node
//! type predicate before the TS2677 assignability relation.
//!
//! Structural rule: a type predicate `p is X` is valid when `X` is assignable to
//! `p`'s declared type. When either is written through a type alias — a
//! `Lazy(DefId)` head (`type A = string`) or a generic-alias `Application`
//! (`type Alias<T> = keyof T`, `type To<T> = T`) — tsc resolves the alias to its
//! body and runs the relation structurally. tsz's function-type-node predicate
//! check ran the relation with a non-resolving resolver, so the alias stayed
//! opaque and the relation spuriously failed with TS2677.
//!
//! Owner: `TypeNodeChecker::check_type_predicate_assignability`
//! (`crates/tsz-checker/src/types/type_node.rs`) now resolves both the asserted
//! and parameter types through the env-aware evaluator before the relation, and
//! does so *before* the type-parameter normalization so an alias that resolves
//! to a type parameter is treated symmetrically with the bare parameter. #14231.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_has_2677(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2677),
        "{label}: expected a TS2677, got {codes:?}"
    );
}

// =============================================================================
// Positive: alias-typed predicates resolve before the relation
// =============================================================================

#[test]
fn generic_alias_application_parameter_resolves() {
    // The reported repro (#14231): `Alias<T> = keyof T`.
    assert_no_errors(
        r#"
type Alias<T> = keyof T;
let g: <T>(p: Alias<T>) => p is keyof T;
export {};
"#,
        "Alias<T> = keyof T parameter resolves before TS2677 relation",
    );
}

#[test]
fn plain_alias_parameter_resolves() {
    assert_no_errors(
        r#"
type A = string;
let g: (p: A) => p is string;
export {};
"#,
        "plain alias parameter (type A = string) resolves",
    );
}

#[test]
fn plain_alias_asserted_type_resolves() {
    assert_no_errors(
        r#"
type A = string;
let g: (p: string) => p is A;
export {};
"#,
        "plain alias asserted type resolves",
    );
}

#[test]
fn identity_generic_alias_resolves_symmetrically() {
    // `To<T> = T`: the alias resolves to the same type parameter as the asserted
    // type, so both sides must be normalized the same way (no spurious TS2677).
    assert_no_errors(
        r#"
type To<T> = T;
let g: <T>(p: To<T>) => p is T;
export {};
"#,
        "identity generic alias To<T> = T resolves symmetrically",
    );
}

#[test]
fn alias_resolution_is_binder_name_independent() {
    assert_no_errors(
        r#"
type KeysOf<U> = keyof U;
let pick: <U>(key: KeysOf<U>) => key is keyof U;
export {};
"#,
        "renamed binders still resolve the alias predicate",
    );
}

// =============================================================================
// Negative: a genuinely non-assignable predicate still reports TS2677.
// =============================================================================

#[test]
fn genuinely_unassignable_predicate_still_errors() {
    assert_has_2677(
        r#"
let bad: (p: string) => p is number;
export {};
"#,
        "p is number with p: string still errors (no over-resolution)",
    );
}
