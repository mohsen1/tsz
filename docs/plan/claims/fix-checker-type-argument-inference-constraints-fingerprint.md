# [WIP] fix(checker): align constrained type argument inference fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-type-argument-inference-constraints-fingerprint`
- **PR**: #3484
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `typeArgumentInferenceWithConstraints.ts` fingerprint-only
conformance mismatch. The expected and actual diagnostic code sets already
match TypeScript (`TS2322`, `TS2344`, `TS2345`, `TS2349`, `TS2403`), so this
slice is scoped to diagnostic message/source display parity for generic call
inference with constrained type arguments.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/tests/type_argument_inference_constraints_fingerprint_tests.rs`
- `crates/tsz-checker/Cargo.toml`

## Verification

- `cargo fmt --all --check`
- `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker --test type_argument_inference_constraints_fingerprint_tests invalid_explicit_type_arg_constraints_suppress_call_argument_cascades --failure-output immediate-final --no-fail-fast`
- `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker --test type_arg_count_mismatch_tests --failure-output immediate-final --no-fail-fast`
- Blocked: `./scripts/conformance/conformance.sh run --profile dev --test-dir tmp-conformance-cases --filter "typeArgumentInferenceWithConstraints" --workers 1 --verbose` could not complete locally because compilation of the CLI/conformance dependency graph was repeatedly terminated while building `tsz-lsp`/`clap_builder`.
