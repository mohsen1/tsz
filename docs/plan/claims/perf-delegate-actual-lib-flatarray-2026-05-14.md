# Claim: Admit FlatArray direct actual-lib alias body

Date: 2026-05-14

## Claim

Admitting `FlatArray` in the direct actual-lib alias-body allowlist removes
another declaration-file `DelegateCrossArenaSymbol` miss family on monorepo-006
with unchanged diagnostics.

## Evidence

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - extends `is_direct_actual_lib_alias_body_admitted` with `FlatArray`.
  - extends mapped utility alias-body proof parity coverage to include
    `FlatArray` (`es2019.array.d.ts`) and assert proof/body/type-param parity
    against child-checker fallback.
- `docs/plan/perf-runs/2026-05-14-delegate-actual-lib-flatarray.md`
  records monorepo-006 attribution evidence:
  - diagnostics unchanged (`10,198`)
  - `DelegateCrossArenaSymbol` children `4 -> 2`
  - `delegate.misses` `4 -> 2`
  - `checker.with_parent_cache_constructed` `4 -> 2`
  - declaration-file residue row `FlatArray` removed.

## Validation

- `CARGO_TARGET_DIR=/private/tmp/tsz-flatarray-target cargo test -p tsz-checker --lib cross_file_direct -- --nocapture`
- `CARGO_TARGET_DIR=/private/tmp/tsz-flatarray-target cargo test -p tsz-checker --test generic_call_inference_tests optional_tuple_generic_param_accepts_required_undefined_union_tuple -- --nocapture`
- `CARGO_TARGET_DIR=/private/tmp/tsz-flatarray-target cargo test -p tsz-checker --test required_constraint_local_alias_tests -- --nocapture`
- `CARGO_TARGET_DIR=/private/tmp/tsz-flatarray-perf-target cargo build -p tsz-cli --bin tsz --release --features perf-tools`
- `TSZ_PERF_COUNTERS=1 /private/tmp/tsz-flatarray-perf-target/release/tsz --noEmit -p /private/tmp/tsz-bench-fixtures/monorepo-006/tsconfig.json --extendedDiagnostics --pretty false --diagnostics-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-flatarray-after-monorepo-006-diag.json --perf-counters-json docs/plan/perf-runs/raw/2026-05-14-delegate-actual-lib-flatarray-after-monorepo-006-pc.json` (expected exit `2`)
