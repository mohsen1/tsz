# fix(checker): report destructuring tuple diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-destructuring-tuple-diagnostics`
- **PR**: #3036
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/destructuringTuple.ts`.
`tsz` currently reports no diagnostics where `tsc` reports `TS2488` and
`TS2769`. This PR will root cause the checker divergence, add focused Rust
regression coverage in the owning crate, and verify the targeted conformance
case.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-tuple-diagnostics.md`
- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-cli/src/driver/check.rs`

## Verification

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-cli es2015_destructuring_reduce_concat_reports_overload_and_iterability`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test lib_resolution_identity_tests test_reduce_empty_array_concat_failure_surfaces_through_destructuring`
- `CARGO_TARGET_DIR=.target/nextest-local cargo check --package tsz-checker --package tsz-cli`
- `./scripts/conformance/conformance.sh run --filter "destructuringTuple" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
