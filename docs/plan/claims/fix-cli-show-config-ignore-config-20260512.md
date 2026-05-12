# [WIP] fix(cli): honor showConfig with ignoreConfig

- **Date**: 2026-05-12
- **Branch**: `fix/cli-show-config-ignore-config-20260512`
- **Issue**: #6010
- **Status**: claim
- **Workstream**: 2 (CLI compatibility bug; user-facing `tsc` parity)

## Intent

Make `tsz --showConfig --ignoreConfig` match `tsc` by printing the resolved
configuration and exiting zero when a `tsconfig.json` exists, instead of
emitting TS5081.

## Files Touched

- CLI `--showConfig` configuration resolution path as needed
- Focused CLI compatibility tests for `--showConfig --ignoreConfig`

## Verification

- Planned: focused `tsz-cli` tests for `--showConfig --ignoreConfig`
- Planned: manual repro from #6010
