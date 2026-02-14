# Performance Report - 2026-02-14

## Scope
This report covers the `performance` branch changes relative to `origin/main`:

1. Binder symbol-name hot path changes (removed unused `name_atom` overhead and map maintenance).
2. Multi-file/lib merge path optimization (`src/parallel.rs` + binder merge path):
   - interned name key lookups (`Atom`) in hot merge maps,
   - declaration dedup switched from repeated `Vec::contains` scans to `FxHashSet`-based seen tracking,
   - wildcard re-export dedup moved to set-based checks.
3. CI/workflows: **no changes**.

## Environment
From `docs/perf/raw/pr-2026-02-14/env.txt`:

- Host: `MacBookPro.fritz.box`
- OS: `Darwin 25.2.0 arm64`
- `rustc`: `1.90.0`
- `cargo`: `1.90.0`

## Validation
- `cargo test -p tsz-binder -p tsz-checker -p tsz-solver` (pass)
- Targeted benchmark pairs run on both:
  - baseline worktree: `/Users/mohsen/code/tsz` (`main`)
  - candidate worktree: `/Users/mohsen/code/tsz/performance` (`performance`)

## Benchmarks (Main vs Performance)
Times below use the middle value of Criterion's `[low mid high]` interval.

| Benchmark | Main | Performance | Delta |
|---|---:|---:|---:|
| `phase_timing/2_parse_bind/100decls_1322lines` | 3.6561 ms | 0.8075 ms | **-77.9%** |
| `phase_timing/2_parse_bind/200decls_2642lines` | 2.4219 ms | 1.8194 ms | **-24.9%** |
| `phase_timing/2_parse_bind/400decls_5282lines` | 15.5280 ms | 16.1950 ms | +4.3% |
| `solver_bench:subtype_object_union_match` | 177.28 µs | 133.31 µs | **-24.8%** |
| `solver_bench:subtype_object_union_miss` | 17.089 µs | 13.756 µs | **-19.5%** |
| `real_world/synthetic_100_classes_full_pipeline` | 1.3557 ms | 1.3072 ms | **-3.6%** |
| `cache_reuse/cold_check/100decls_1322lines` | 2.0375 ms | 4.3428 ms | +113.1% |

Raw artifacts:
- `docs/perf/raw/pr-2026-02-14/main-phase-bind-100.txt`
- `docs/perf/raw/pr-2026-02-14/perf-phase-bind-100.txt`
- `docs/perf/raw/pr-2026-02-14/main-phase-bind-200.txt`
- `docs/perf/raw/pr-2026-02-14/perf-phase-bind-200.txt`
- `docs/perf/raw/pr-2026-02-14/main-phase-bind-400.txt`
- `docs/perf/raw/pr-2026-02-14/perf-phase-bind-400.txt`
- `docs/perf/raw/pr-2026-02-14/main-solver-union-match-rerun.txt`
- `docs/perf/raw/pr-2026-02-14/perf-solver-union-match-rerun-after-restore.txt`
- `docs/perf/raw/pr-2026-02-14/main-solver-union-miss-rerun.txt`
- `docs/perf/raw/pr-2026-02-14/perf-solver-union-miss-rerun-after-restore.txt`
- `docs/perf/raw/pr-2026-02-14/main-realworld-full-100classes-rerun.txt`
- `docs/perf/raw/pr-2026-02-14/perf-realworld-full-100classes-rerun.txt`
- `docs/perf/raw/pr-2026-02-14/main-cache-cold-100-rerun.txt`
- `docs/perf/raw/pr-2026-02-14/perf-cache-cold-100-rerun-after-restore.txt`

## bench-vs-tsgo snapshot
Quick-mode snapshots exist for both trees:
- `docs/perf/raw/bench-vs-tsgo-main.clean.txt`
- `docs/perf/raw/bench-vs-tsgo-performance.clean.txt`

These runs are not a direct main-vs-performance apples-to-apples microbenchmark (they compare `tsz` vs `tsgo` per tree), but both show `tsz` consistently outperforming `tsgo` on the executed quick subset.

## Conformance (main vs performance)
From existing recorded runs (summarized in `docs/perf/raw/pr-2026-02-14/conformance-summary.txt`):

- Main: `8208/12546` passed (`65.4%`)
- Performance: `8206/12546` passed (`65.4%`)
- Net: **-2 tests** on current `performance` snapshot.
- Divergence lists:
  - main-only passes: 20 files
  - performance-only passes: 22 files

## Interpretation
- The binder/merge-focused changes deliver clear wins on several bind/merge-adjacent and solver subtype hot paths.
- Results are mixed at larger scale (`parse_bind` 400 case) and delegation-heavy cache benchmark (`cache_reuse/cold_check`), so further tuning is still needed.
- No behavioral/diagnostic parity guarantees are claimed from this performance-only pass; conformance currently regresses by 2 tests vs main.

## Follow-up candidates
1. Investigate `cache_reuse/cold_check` regression with per-context cache lifecycle profiling.
2. Re-run the larger benchmark matrix on an isolated/quiet host and pin CPU governor settings.
3. Triage the 20/22 conformance divergence files and recover the -2 pass delta before merge.
