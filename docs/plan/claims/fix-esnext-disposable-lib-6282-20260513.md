# fix(cli): load esnext disposable symbols from --lib esnext

- **Date**: 2026-05-13
- **Branch**: `fix-esnext-disposable-lib-6282-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / lib compatibility)

## Intent

Issue #6282 reports that `--lib esnext` still emits TS2339 for `Symbol.dispose` and `Symbol.asyncDispose` in using declarations. The lib asset already contains `esnext.disposable.d.ts`, so this claim targets lib resolution/loading rather than changing the declaration text. The fix should keep lib ownership in config/driver loading and add a focused CLI regression for the reported repro.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` (expected, lib resolution if needed)
- `crates/tsz-core/src/config/mod.rs` or lib-resolution tests (expected, if transitive expansion is the root cause)
- `crates/tsz-cli/tests/tsc_compat_tests.rs` or driver tests (expected regression)

## Verification

- TBD
