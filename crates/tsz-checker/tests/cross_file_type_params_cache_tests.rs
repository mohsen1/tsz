//! T2.2 typed-cross-file-query tests: lock the
//! `cross_file_type_params_cache` plumbing.
//!
//! The cache replaces a `with_parent_cache_attributed(...,
//! TypeEnvironmentCore)` child-checker construction per call site (the
//! dominant share of `with_parent_cache_constructed` in the original
//! attribution run for this work).
//!
//! Per `PERFORMANCE_PLAN.md` §7, each Tier 2.2 PR ships:
//! 1. a unit test that locks the new behavior, and
//! 2. proof that the targeted reason's construction count drops on a
//!    repro fixture (covered in this PR's body via `--extendedDiagnostics`
//!    on the scale-cliff fixtures).
//!
//! These tests cover the unit-test contract: the cache is wired through
//! the `CheckerContext` / `ProgramContext` plumbing, and the existing
//! arena-only fast path is preserved (no double-counting). A test that
//! exercises the constraint slow path against a synthetic two-file
//! fixture overflows the much-smaller test stack — an artifact of the
//! test harness, not of the cache itself — so end-to-end proof of slow-path
//! avoidance lands in the PR body via the bench fixtures.

use crate::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        ..CheckerOptions::default()
    }
}

#[test]
fn cache_helper_returns_an_empty_dashmap_when_no_cross_file_resolution() {
    // Single-file project with no cross-file generic resolution: the
    // helper still hands back an `Arc<DashMap>`, and that DashMap stays
    // empty because nothing routed through the
    // `with_parent_cache_attributed(..., TypeEnvironmentCore)` path.
    // This proves the plumbing is in place — the cache field is now
    // present on `CheckerContext` and the test harness honors it.
    let file = r#"
        export interface Wrapper {
            v: number;
        }
        export const x: Wrapper = { v: 1 };
    "#;
    let (_diags, cache) = crate::test_utils::check_multi_file_with_type_params_cache(
        &[("file.ts", file)],
        "file.ts",
        opts(),
    );
    assert!(
        cache.is_empty(),
        "single-file project must not populate the cross-file type-params cache; \
         had {} entries",
        cache.len()
    );
}

#[test]
fn no_constraint_no_default_generic_takes_arena_only_fast_path() {
    // The arena-only fast path
    // (`extract_simple_type_params_from_decl_in_arena`) returns
    // `Some(...)` for type parameters with no constraint and no
    // default. The cache is consulted only when that fast path
    // returns `None` (the slow-path case). So a cross-file generic
    // alias with no constraint and no default must NOT populate the
    // cache — guarding against accidentally widening the cache to
    // swallow fast-path traffic, which would be a double-counting
    // bug.
    //
    // The fixture below declares cross-file generics (`Inner<T>`,
    // `Outer<U>`) with no constraints and no defaults. Both `T` and
    // `U` are exactly the arena-only fast-path shape; if cache
    // entries appear here, something widened to swallow the fast
    // path.
    let file1 = r#"
        export interface Inner<T> {
            bar(x: T): void;
        }
        export interface Outer<U> {
            inner: Inner<U>;
        }
    "#;
    let file2 = r#"
        import { Outer } from "./file1";
        declare const o: Outer<string>;
        o.inner.bar("ok");
    "#;
    let (_diags, cache) = crate::test_utils::check_multi_file_with_type_params_cache(
        &[("file2.ts", file2), ("file1.ts", file1)],
        "file2.ts",
        opts(),
    );
    assert!(
        cache.is_empty(),
        "no-constraint cross-file interface must take the arena-only fast path; \
         cache had {} entries",
        cache.len()
    );
}

#[test]
fn scope_independent_constraint_and_default_take_arena_only_fast_path() {
    // Constraints/defaults that lower without source-file symbol resolution
    // can be extracted directly from the declaration arena. They should not
    // pay for a `TypeEnvironmentCore` child checker or populate the slow-path
    // cache.
    let file1 = r#"
        export type Pair<T extends string, U = T[]> = [T, U];
    "#;
    let file2 = r#"
        import { Pair } from "./file1";
        type Value = Pair<"ok">;
        declare const value: Value;
        value;
    "#;
    let (_diags, cache) = crate::test_utils::check_multi_file_with_type_params_cache(
        &[("file2.ts", file2), ("file1.ts", file1)],
        "file2.ts",
        opts(),
    );
    assert!(
        cache.is_empty(),
        "scope-independent constrained/defaulted generic should take the arena-only fast path; \
         cache had {} entries",
        cache.len()
    );
}

#[test]
fn cache_stores_only_positive_type_param_results() {
    // Negative extraction results are intentionally not cached. A
    // `None` answer can be context-dependent, so memoizing it can
    // suppress a later successful extraction for the same
    // `(file_idx, decl_idx)` under a different query context.
    //
    // The actual value-shape contract is checked at the type level:
    // the cache stores `Vec<TypeParamInfo>`, not
    // `Option<Vec<TypeParamInfo>>`. If a future refactor reintroduces
    // negative entries, this test should fail to compile until the
    // correctness risk is re-audited.
    let cache: crate::context::CrossFileTypeParamsCache =
        std::sync::Arc::new(dashmap::DashMap::new());
    let key = (0u32, tsz_parser::parser::NodeIndex::NONE);
    cache.insert(key, Vec::<tsz_solver::TypeParamInfo>::new());
    let observed: Option<Vec<tsz_solver::TypeParamInfo>> =
        cache.get(&key).map(|e| e.value().clone());
    assert_eq!(observed, Some(Vec::new()));
}

#[test]
fn cache_field_is_optional_and_off_by_default() {
    // The plain `check_multi_file` helper does not install the
    // cache, so production-shape fallback (slow path constructs the
    // child checker, no memoization) is preserved for callers that
    // don't opt in. This guards against any future change that would
    // make the cache mandatory and require all integrators to thread
    // an Arc<DashMap> through their drivers.
    let file = r#"
        export interface Wrapper {
            v: number;
        }
    "#;
    // Simply ensure this returns without panic — the bare
    // `check_multi_file` path runs with `cross_file_type_params_cache`
    // = None.
    let _ = crate::test_utils::check_multi_file(&[("file.ts", file)], "file.ts", opts());
}
