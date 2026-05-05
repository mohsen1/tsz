# fix(checker): align generic return inference fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-infer-generic-return-fingerprints`
- **PR**: #3123
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/inferFromGenericFunctionReturnTypes3.ts`.
The previous TS2769 mismatch is merged, but the test still reports matching
diagnostic codes with incorrect spans/messages around literal-preserving
generic return inference and the `bar(() => ... ? [{ state: State.A }] :
[{ state: State.B }])` callback.

## Files Touched

- `docs/plan/claims/fix-checker-infer-generic-return-fingerprints.md`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/checkers/call_context.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/tests/generic_return_fingerprint_tests.rs`
- `crates/tsz-core/src/config/mod.rs`
- `crates/tsz-core/src/module_resolver/request_types.rs`
- `crates/tsz-solver/src/inference/infer_resolve.rs`

## Verification

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test generic_return_fingerprint_tests`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test generic_call_inference_tests contextual_nested_generator_return_inference_drops_stale_ts2345`
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test call_resolution_regression_tests inheritance_merged_overload_pairs_last_source_sig_for_inference`
- `./scripts/conformance/conformance.sh run --filter "inferFromGenericFunctionReturnTypes3" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
- `PATH="$HOME/.cargo/bin:$PATH" scripts/githooks/pre-commit`
  - `All pre-commit checks passed!`
