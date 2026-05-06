# fix(checker): finish constrained type argument inference fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-type-argument-inference-window-fingerprints`
- **PR**: #3658
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Finish the `typeArgumentInferenceWithConstraints.ts` conformance slice started
in PR #3484. The remaining mismatch came from lib-backed generic constraints
collapsing during explicit type-argument validation and callback contextual
return checking, plus a few display fingerprints in the same legacy case.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/src/state/type_resolution/module.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/tests/type_argument_inference_constraints_fingerprint_tests.rs`
- `crates/tsz-cli/src/driver/core.rs` (pre-existing clippy cleanup required by the hook)
- `crates/tsz-core/src/config/mod.rs` (pre-existing clippy cleanup required by the hook)

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/final-typearg CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo nextest run -p tsz-checker --test type_argument_inference_constraints_fingerprint_tests --failure-output immediate-final --no-fail-fast`
  - `3 tests run: 3 passed, 0 skipped`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/final-typearg CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo build -p tsz-cli -p tsz-conformance`
- `/Users/mohsen/code/tsz-build-targets/final-typearg/debug/tsz-conformance --test-dir /Users/mohsen/code/tsz-worktrees/origin-main-20260505-7/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-build-targets/final-typearg/debug/tsz --workers 1 --print-test --print-fingerprints --verbose --filter typeArgumentInferenceWithConstraints`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- Pre-commit hook with `CARGO_INCREMENTAL=0` passed formatting, affected-crate
  clippy, full-workspace clippy parity after the cleanup lines above, wasm32
  warnings, and architecture guardrails. The final affected-crate test
  compilation step was not used as evidence because local test-binary linking
  exhausted disk; the focused nextest run above was rerun on the committed tree.

Note: `./scripts/conformance/conformance.sh run --profile debug ...` now exits
before running because Cargo reserves the `debug` profile name. The same
filtered run was verified directly with the debug-profile runner binary built
by the harness.
