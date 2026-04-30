# [WIP] fix(checker): align mapped type error fingerprints

- **Date**: 2026-04-29
- **Branch**: `codex/conformance-pick-20260430`
- **PR**: #1832
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

This PR targets the `mappedTypeErrors.ts` conformance failure selected by
`scripts/session/quick-pick.sh`. The diagnostic code set already matches tsc,
so the intended scope is fingerprint parity: message text, count grouping, or
anchor/location behavior for mapped type diagnostics.

## Files Touched

- `docs/plan/claims/codex-conformance-pick-20260430.md` (claim/status)
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/error_reporter/assignability_helpers.rs`
- `crates/tsz-checker/src/error_reporter/properties.rs`
- `crates/tsz-checker/src/state/state_checking/property.rs`
- `crates/tsz-checker/src/state/state_checking/mapped_object_literals.rs`
- `crates/tsz-checker/src/state/state_checking/mod.rs`
- `crates/tsz-checker/src/state/type_resolution/constructors.rs`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/tests/mapped_type_errors_conformance_tests.rs`
- `crates/tsz-solver/src/inference/infer_matching.rs`
- `crates/tsz-solver/src/operations/constraints/walker.rs`
- `crates/tsz-solver/src/relations/compat_overrides.rs`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-solver --lib`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run -p tsz-checker --test mapped_type_errors_conformance_tests --no-fail-fast`
- `./scripts/conformance/conformance.sh run --filter "mappedTypeErrors" --verbose`
  - `FINAL RESULTS: 2/2 passed (100.0%)`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
  - `FINAL RESULTS: 12267/12582 passed (97.5%)`
