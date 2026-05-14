# Claim: Admit Intl info interfaces in direct actual-lib path

Date: 2026-05-14

## Claim

Admitting `Intl.TextInfo` and `Intl.WeekInfo` in the direct actual-lib
namespace interface path removes the final two declaration-file
`DelegateCrossArenaSymbol` misses on regenerated monorepo-006 with unchanged
diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends the narrow Intl namespace interface admission with `TextInfo` and
    `WeekInfo`.
  - extends the selected value-interface proof test with an in-test
    `esnext.intl.d.ts` lib fixture containing these interfaces.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-intl-info-interfaces.md`
  records monorepo-006 attribution evidence against the #6820 baseline:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `2 -> 0`
  - `delegate.misses` `2 -> 0`
  - `checker.with_parent_cache_constructed` `2 -> 0`
  - declaration-file residue rows `TextInfo` and `WeekInfo` removed.

## Validation

- `cargo test -p tsz-checker --lib direct_actual_lib_symbol_type_handles_selected_value_interfaces -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /Users/mohsen/.cache/tsz-target/release/tsz --extendedDiagnostics --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-intl-info-interfaces-after-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-intl-info-interfaces-after-monorepo-006-pc.json` (expected diagnostics exit)
