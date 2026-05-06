# fix(checker): align combined rest parameter diagnostics

- **Date**: 2026-05-06
- **Branch**: `codex/signature-rest-combining-20260506`
- **PR**: #3633
- **Status**: implemented
- **Workstream**: 1 (Conformance)

## Intent

Fix the `TypeScript/tests/cases/compiler/signatureCombiningRestParameters5.ts`
conformance mismatch. The current filtered run emits `TS2345` for the first
array argument with a literal array display (`true[]`) and misses the second
combined-signature rest parameter diagnostic.

The expected impact is a one-test conformance pass-rate increase without
changing unrelated overload or rest-parameter diagnostics.

## Files Touched

- `docs/plan/claims/codex-signature-rest-combining-20260506.md`
- `crates/tsz-checker/src/checkers/call_checker/diagnostics.rs`
- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/union_index_access_function_application_param_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests signature_combining_rest_parameters_5_reports_both_rest_argument_mismatches -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests signature_combining_rest_parameters_4_preserves_intersection_display_order -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests -- --nocapture`
- `git diff --check`
- `rg -n "eprintln!|dbg!|println!" ...` (no matches)

Conformance was attempted with the pinned TypeScript fixture, but the
`cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` step failed
with `No space left on device` before the conformance binary could be run.
