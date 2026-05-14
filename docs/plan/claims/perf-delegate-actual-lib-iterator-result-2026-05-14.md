# Claim: Admit IteratorResult direct actual-lib alias body

Date: 2026-05-14

## Claim

Admitting `IteratorResult` in the direct actual-lib alias-body allowlist removes
the remaining two `IteratorResult` declaration-file `DelegateCrossArenaSymbol`
misses on regenerated monorepo-006 with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_lib_alias_body_admitted` with `IteratorResult`.
  - updates the mapped utility fallback proof test so `IteratorResult` resolves
    through the direct alias-body path while preserving its two type
    parameters.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-iterator-result.md`
  records regenerated monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `4 -> 2`
  - `delegate.misses` `4 -> 2`
  - `checker.with_parent_cache_constructed` `4 -> 2`
  - declaration-file residue row `IteratorResult` removed (`2 -> 0`).

## Validation

- `cargo fmt --all --check`
- `cargo test -p tsz-checker --lib direct_actual_lib_alias_proof_matches_mapped_utility_fallback_bodies -- --nocapture`
- `cargo test -p tsz-checker --test generic_alias_assignability_pollution_tests -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests iterator_result -- --nocapture`
- `cargo test -p tsz-checker --test ts2322_tests flatarray -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_array_flat_lib_alias_return_does_not_emit_ts5088 -- --nocapture`
- `cargo test -p tsz-checker --test generator_return_type_widening_tests -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-iterres-baseline-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-baseline-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-baseline-monorepo-006-pc.json` (expected diagnostics exit)
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-iterres-after-target/release/tsz --noEmit -p scripts/bench/scale-cliff/fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-iterator-result-after-monorepo-006-pc.json` (expected diagnostics exit)
