# [WIP] fix(cli): validate invalid locale values

- **Date**: 2026-05-12
- **Branch**: `fix/cli-invalid-locale-validation-20260512`
- **Issue**: #6009
- **Status**: claim
- **Workstream**: 2 (existing CLI bug; user-facing diagnostic parity)

## Intent

Make `tsz --locale <invalid>` match `tsc` by emitting TS6048 and exiting
non-zero instead of silently falling back to the default English locale.

## Files Touched

- `crates/tsz-cli/src/localization/locale.rs`
- CLI startup/driver validation path as needed
- CLI tests for invalid locale handling

## Verification

- Planned: focused `tsz-cli` tests for invalid `--locale`
- Planned: manual repro from #6009 with explicit root file and with
  `tsconfig.json` discovery
