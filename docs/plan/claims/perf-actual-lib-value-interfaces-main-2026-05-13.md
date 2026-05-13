# perf(checker): admit selected actual-lib value interfaces

## Claim

The direct actual-lib symbol path can safely admit a narrow set of
value-bearing bundled-lib interfaces (`Function`, `Object`, `RegExp`) without
opening the broader generic alias shortcut that previously failed conformance.

## Implementation

- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
  - adds an allowlist for the three selected value-bearing interface names.
  - keeps generic utility aliases and `PropertyKey` on the fallback path.
  - preserves the existing `(TypeId, Vec<TypeParamInfo>)` lib delegation cache
    contract.

## Verification

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo test -p tsz-checker --lib direct_actual_lib_symbol_type -- --nocapture`
- `CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_file_size_ceiling -- --nocapture`
- `CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo check -p tsz-checker`
- `CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo clippy -p tsz-checker --all-targets --all-features -- -D warnings`
- `CARGO_TARGET_DIR=.target-perf-refresh-20260513 cargo build -p tsz-cli --release --features perf-tools`
- monorepo-006 attribution run:
  - diagnostics unchanged: `10,198 -> 10,198`
  - `checker.with_parent_cache_constructed`: `29 -> 26`
  - `DelegateCrossArenaSymbol`: `26 -> 23`
  - `delegate.misses`: `28 -> 25`
- monorepo-006 timing-mode run:
  - diagnostics unchanged: `10,198 -> 10,198`
  - `check_ms`: `166,369.995 -> 158,218.144875`
  - `total_ms`: `170,182.516291 -> 160,254.267208`
  - `rss_peak_bytes`: `3,018,948,608 -> 3,019,358,208`

Perf-run note:
[`perf-runs/2026-05-13-delegate-actual-lib-value-interfaces-main.md`](../perf-runs/2026-05-13-delegate-actual-lib-value-interfaces-main.md).
