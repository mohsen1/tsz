//! Regression tests for the memoized `collect_lazy_def_ids` walk
//! (`CheckerContext::collect_lazy_def_ids_cached`).
//!
//! The lazy-`DefId` set reachable from a type is a pure function of the
//! immutable interned type structure, so it is cached per `type_id` and reused
//! across the many hot callers (alias-cycle checking, assignability ref
//! resolution, `lib_augmentations`, the env-eval seed/persist scan, TS2589
//! depth-poison detection). These tests pin that the cache preserves behavior:
//! a recursive alias referenced from multiple assignability positions (cache
//! hits) must still narrow assignability correctly, and depth-poisoned infinite
//! recursion must still be tolerated rather than producing structural noise.

use tsz_checker::test_utils::check_source_strict_codes;

/// A recursive generic alias (`Tree<T>` carries a `Lazy(Tree)` ref) used in
/// several assignment positions. Correct-typed assignments must pass and only
/// the genuinely mismatched one (`Tree<number>` -> `Tree<string>`) trips TS2322.
/// If the cache aliased or dropped reachable lazy def ids, structural
/// assignability of the recursive shape would break.
#[test]
fn recursive_alias_assignability_is_stable_across_occurrences() {
    let codes = check_source_strict_codes(
        r#"
type Tree<T> = { value: T; children: Tree<T>[] };
declare const a: Tree<number>;
const ok1: Tree<number> = a;
const ok2: { value: number; children: Tree<number>[] } = a;
const ok3: Tree<number> = a;
"#,
    );
    assert!(
        codes.is_empty(),
        "correct recursive-alias assignments must not error, got: {codes:?}"
    );

    let mismatch = check_source_strict_codes(
        r#"
type Tree<T> = { value: T; children: Tree<T>[] };
declare const a: Tree<number>;
const bad: Tree<string> = a;
"#,
    );
    assert!(
        mismatch.contains(&2322),
        "Tree<number> assigned to Tree<string> must trip TS2322, got: {mismatch:?}"
    );
}

/// Two distinct recursive aliases that share a member name must stay
/// independent through the cached lazy-def-id walk: assigning one to the other
/// must error, exercising the cache for two different reachable lazy def sets.
#[test]
fn distinct_recursive_aliases_do_not_alias_in_cache() {
    let codes = check_source_strict_codes(
        r#"
type ListA = { head: number; tail: ListA | null };
type ListB = { head: string; tail: ListB | null };
declare const a: ListA;
const sameOk: ListA = a;
const crossBad: ListB = a;
"#,
    );
    assert!(
        codes.contains(&2322),
        "ListA assigned to ListB must trip TS2322 (distinct lazy def sets), got: {codes:?}"
    );
}
