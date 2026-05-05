# [WIP] fix(checker): preserve generic JSX spread source display

- **Date**: 2026-05-05
- **Branch**: `fix/jsx-generic-spread-source-display`
- **PR**: #2756
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the remaining fingerprint-only divergence in
`tsxAttributeResolution5.tsx`: for generic JSX spread attributes, tsc reports
the failing source type as the type parameter surface (`T`), while tsz currently
renders the evaluated intersection with the constraint (`T & { ... }`). The
fix should keep relation checking routed through the existing JSX spread and
assignability boundaries, and narrow only the final diagnostic display surface.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/spread.rs` (expected)
- `crates/tsz-checker/tests/jsx_spread_assignability_suppresses_ts2741.rs` (expected)

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run --package tsz-checker --test jsx_spread_assignability_suppresses_ts2741` (4 tests pass)
- `cargo nextest run --package tsz-checker --lib` (3333 tests pass, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5622 tests pass, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "tsxAttributeResolution5" --verbose` (1/1 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12436 -> 12439, +3 net; 0 regressions reported)
