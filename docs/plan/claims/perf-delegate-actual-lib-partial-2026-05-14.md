# Claim: Admit Partial direct actual-lib alias body

Date: 2026-05-14

## Claim

Admitting `Partial` in the direct actual-lib alias-body allowlist removes one
more declaration-file `DelegateCrossArenaSymbol` miss on monorepo-006 with
unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_lib_alias_body_admitted` with `Partial`.
  - updates direct alias-body tests so `Partial` is expected to resolve through
    direct alias-body lowering while preserving one generic type parameter.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-partial.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `5 -> 4`
  - `delegate.misses` `5 -> 4`
  - `checker.with_parent_cache_constructed` `5 -> 4`
  - declaration-file residue row `Partial` removed.

## Validation

- `cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests optional_tuple_generic_param_accepts_required_undefined_union_tuple -- --nocapture`
- `cargo test -p tsz-checker --test required_constraint_local_alias_tests -- --nocapture`
- `cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-partial-baseline-target-b/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-baseline-b-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-baseline-b-monorepo-006-pc.json` (expected exit `2`)
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-partial-after-target-b/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-partial-after-b-monorepo-006-pc.json` (expected exit `2`)
