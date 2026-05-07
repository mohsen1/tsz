# checker: don't suppress TS2345 when callback has more params than target

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-BxadF`
- **PR**: TBD
- **Status**: claim
- **Workstream**: TS2345 parity (issue #4027)

## Intent

Fix issue #4027: the assignability layer suppresses TS2345 for any callback
argument whose parameters are unannotated, even when the target callback
signature has fewer parameters than the source. The "Target signature provides
too few arguments" mismatch must still surface — contextual typing cannot
supply types for parameters the target signature does not have.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/assignability/tests.rs` (new unit test)

## Verification

- `cargo nextest run -p tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --filter ...` (no regressions)
