# [WIP] fix(checker): align strict optional property fingerprints

- **Date**: 2026-04-28
- **Branch**: `fix/checker-strict-optional-properties-fingerprint`
- **PR**: #1698
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`, a fingerprint-only conformance failure. This PR aligns the TS2375/TS2412 exact-optional target display so optional target properties preserve their surface syntax instead of appending synthetic `| undefined`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/assignability.rs` (~10 LOC)
- `crates/tsz-checker/tests/ts2322_tests.rs` (~20 LOC)
- `crates/tsz-solver/src/diagnostics/format/mod.rs` (~7 LOC)

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run -p tsz-checker exact_optional_property` (5 tests pass)
- `cargo nextest run -p tsz-solver optional_property` (64 tests pass)
- `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties1" --verbose` (still fingerprint-only; TS2375 target-display mismatch removed, remaining tuple/source-anchor mismatches are separate)
- `./scripts/conformance/conformance.sh run --max 200` (199/200 pass; `aliasOnMergedModuleInterface.ts` remains failing in the sampled window)
