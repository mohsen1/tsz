# perf(checker,common,cli): T2.2 typed cross-file type-parameter memo

- **Date**: 2026-05-10
- **Branch**: `perf/t2.2-typeparam-memo-2026-05-10` (pending rename from `perf/t0.4-…`)
- **PR**: TBD
- **Status**: claim
- **Workstream**: `PERFORMANCE_PLAN.md` Tier 2.2 — first migrated `CheckerCreationReason`

## Intent

Per the 2026-05-10 attribution decision record (#4952 / `docs/plan/perf-runs/2026-05-10-scale-cliff-summary.md`), `TypeEnvironmentCore` is the dominant share (~84 %) of `with_parent_cache_constructed` on the cliff fixtures. This PR migrates the two `with_parent_cache_attributed(..., TypeEnvironmentCore)` call sites in `crates/tsz-checker/src/state/type_environment/core.rs` to a memoized typed-cross-file query, keyed structurally by `(target_file_idx, decl_idx)`.

## Approach

1. Add `pub type CrossFileTypeParamsCache = Arc<DashMap<(u32, NodeIndex), Vec<TypeParamInfo>>>` next to `CheckerContext`.
2. Add an optional `cross_file_type_params_cache` field to both `CheckerContext` and `ProjectEnv`. Driver populates it (`Some(Arc::new(DashMap::new()))`); test paths leave it `None` (cache disabled, slow path runs untouched).
3. At each `TypeEnvironmentCore` site, look up the cache *before* constructing the child checker. On hit, increment `cross_file_type_params_cache_hits`, return the cached `Vec<TypeParamInfo>`, never construct the child. On miss, run the slow path as today, then `or_insert` the result and increment `cross_file_type_params_cache_misses`.
4. Counters surfaced in the T0.3 JSON snapshot (`delegate.cross_file_type_params_cache_hits` / `..._misses`).
5. Unit tests that lock the plumbing and the fast-path-preservation invariant (constraint-bearing slow-path proof lives in the PR body — synthetic two-file constraint repros overflow the test stack).

## Verification

- `cargo build -p tsz-checker -p tsz-cli` clean.
- `cargo nextest run -p tsz-checker --lib -E 'test(cross_file_type_params_cache)'` — 3/3 pass.
- `cargo clippy -p tsz-checker -p tsz-cli -p tsz-common --all-targets -- -D warnings` clean.
- Conformance smoke (`--max 50`): no regressions vs baseline.
- `monorepo-006` cliff fixture (`TSZ_PERF_COUNTERS=1 ... --extendedDiagnostics`): cache populates with 14 entries; 0 hits in single run (cache value compounds in projects with deep constraint reuse). No diagnostic delta.

## Out of scope

- Wider migrations (DelegateCrossArenaSymbol — next-biggest at ~17 %).
- Lifetime split (Tier 2.1).
- Splitting `PhaseTimings` for `module_resolution` / `source_discovery` (T0 follow-up gap).
- Wiring `interner.intern_calls` / `lock_wait_histogram_ns` (T0 follow-up gap).
