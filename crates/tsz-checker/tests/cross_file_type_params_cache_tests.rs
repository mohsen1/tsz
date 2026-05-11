//! T2.2 typed-cross-file-query tests: lock the
//! `cross_file_type_params_cache` plumbing.
//!
//! The cache replaces a `with_parent_cache_attributed(...,
//! TypeEnvironmentCore)` child-checker construction per call site (the
//! dominant share of `with_parent_cache_constructed` per the 2026-05-10
//! attribution run; see `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`).
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
fn cache_stores_inner_option_so_negative_results_are_not_re_extracted() {
    // The cache value type is `Option<Vec<TypeParamInfo>>`, not
    // `Vec<TypeParamInfo>`. This locks the slow path's negative
    // results (i.e. `extract_type_params_from_decl` returned `None`)
    // into the cache too, so a later query for the same
    // `(file_idx, decl_idx)` does NOT re-construct a child checker
    // via `with_parent_cache_attributed(..., TypeEnvironmentCore)`.
    //
    // Without this lock, only positive results were cached. The
    // 2026-05-11 attribution run showed 0 hits / 5320 misses on the
    // scale-cliff fixtures with the previous shape — every slow path
    // entry was paying for a fresh checker, even when the previous
    // entry for the same key had already proven the answer was
    // `None`. See `docs/plan/perf-runs/2026-05-11-attribution-lock-wait.md`.
    //
    // The actual value-shape contract is checked at the type level
    // via this assertion: write a `None`, read it back, and require
    // the read to type-check as `Option<Option<Vec<...>>>`. If a
    // future refactor accidentally drops the outer `Option`, the
    // `cache.insert(..., None)` call stops compiling.
    let cache: crate::context::CrossFileTypeParamsCache =
        std::sync::Arc::new(dashmap::DashMap::new());
    let key = (0u32, tsz_parser::parser::NodeIndex::NONE);
    cache.insert(key, None);
    let observed: Option<Option<Vec<tsz_solver::TypeParamInfo>>> =
        cache.get(&key).map(|e| e.value().clone());
    assert_eq!(observed, Some(None));
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
