# [WIP] fix(conformance): align generic function inference diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/generic-function-inference1-20260505`
- **PR**: #3047
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `genericFunctionInference1.ts` conformance mismatch. The current
fingerprint expects TS2345 but tsz also emits TS2322 and TS2362, so the work
will identify whether the extra diagnostics come from generic inference,
contextual typing, or arithmetic operand checking after failed inference.

## Files Touched

- TBD after investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "genericFunctionInference1" --verbose`
- focused Rust unit tests in the owning crate
- `cargo build --profile dist-fast --bin tsz`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
