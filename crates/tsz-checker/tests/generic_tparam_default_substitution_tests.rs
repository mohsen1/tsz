//! Tests that lock the rule from issue #5878:
//!
//! > When a type-parameter slot is filled from its declared default, the
//! > substitution map for prior bound parameters MUST be applied to the
//! > default's TypeId before it is used as the effective type for the slot.
//!
//! The bare-reference case (`U = T`) already works; the bug manifests when
//! the default wraps the prior type parameter in a compound `TypeData`
//! (`Array<T>`, `Promise<T>`, an application, etc.).

use tsz_checker::test_utils::check_source_codes;

/// Direct repro from #5878. `V`'s default `T[]` must substitute
/// `T → string` so `["a", "b"]: string[]` matches `items: V`.
/// No TS2322 expected.
#[test]
#[ignore = "issue #5878 — pending checker-layer fix; solver from_args works in isolation"]
fn generic_default_array_of_prior_param_substitutes() {
    let diags = check_source_codes(
        "type Container<T, V = T[]> = { value: T; items: V };\n\
         const c1: Container<string> = { value: \"hello\", items: [\"a\", \"b\"] };\n",
    );
    assert!(
        !diags.contains(&2322),
        "Generic default `V = T[]` must substitute T's binding; got TS2322. \
         Full diags: {:?}",
        diags.to_vec(),
    );
}

/// CLAUDE.md §25 anti-hardcoding: the rule must not depend on the spelling
/// of the type-parameter names. Same structural shape, different names.
#[test]
#[ignore = "issue #5878 — pending checker-layer fix; solver from_args works in isolation"]
fn generic_default_array_substitution_independent_of_param_names() {
    let diags = check_source_codes(
        "type Holder<K, P = K[]> = { key: K; rest: P };\n\
         const h: Holder<number> = { key: 1, rest: [2, 3] };\n",
    );
    assert!(
        !diags.contains(&2322),
        "Default `P = K[]` must substitute K's binding regardless of names; \
         got TS2322. Full diags: {:?}",
        diags.to_vec(),
    );
}

/// Default referencing a prior param wrapped in a tuple (`[T]`). Tuples
/// are a different `TypeData` variant than arrays, so this exercises the
/// same rule against the tuple-walking path.
#[test]
#[ignore = "issue #5878 — pending checker-layer fix; solver from_args works in isolation"]
fn generic_default_tuple_of_prior_param_substitutes() {
    let diags = check_source_codes(
        "type Pair<T, V = [T]> = { lhs: T; pair: V };\n\
         const p: Pair<boolean> = { lhs: true, pair: [false] };\n",
    );
    assert!(
        !diags.contains(&2322),
        "Default `V = [T]` (tuple) must substitute T's binding; got TS2322. \
         Full diags: {:?}",
        diags.to_vec(),
    );
}
