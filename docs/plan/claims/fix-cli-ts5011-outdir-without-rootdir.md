# fix(cli): emit TS5011 when outDir is set without rootDir

- **Date**: 2026-05-07
- **Branch**: `fix/cli-ts5011-outdir-without-rootdir`
- **PR**: TBD
- **Status**: claim
- **Workstream**: CLI/config diagnostic parity

## Intent

Issue #3822: `tsc` reports `TS5011` when a project sets `outDir`, omits
`rootDir`, and the inferred common source directory differs from the
config directory (so the output layout would change). `tsz` currently
emits files instead of warning. Add the implicit-common-source-directory
detection and emit `TS5011` from the CLI driver alongside the existing
`TS6059` rootDir-coverage check.

The diagnostic message string is already registered in
`tsz_common::diagnostics::data` (constant
`THE_COMMON_SOURCE_DIRECTORY_OF_IS_THE_ROOTDIR_SETTING_MUST_BE_EXPLICITLY_SET_TO`,
code 5011); only the emission path is missing.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` (~60 LOC: helpers + emission block)
- `crates/tsz-cli/tests/driver_tests.rs` (~80 LOC: fires/non-fires tests)

## Verification

- `cargo nextest run -p tsz-cli --test driver_tests` (TS5011 tests pass)
- No conformance tests expect TS5011, so no conformance impact expected.
