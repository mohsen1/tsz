# fix(cli): honor showConfig with ignoreConfig

- **Date**: 2026-05-12
- **Branch**: `fix/cli-show-config-ignore-config-20260512`
- **Issue**: #6010
- **Status**: ready
- **Workstream**: 2 (CLI compatibility bug; user-facing `tsc` parity)

## Intent

Make `tsz --showConfig --ignoreConfig` match `tsc` by printing the resolved
configuration and exiting zero when a `tsconfig.json` exists, instead of
emitting TS5081.

## Files Touched

- CLI `--showConfig` configuration resolution path as needed
- Focused CLI compatibility tests for `--showConfig --ignoreConfig`

## Verification

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli show_config_ignore_config_without_files_loads_discovered_tsconfig`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli show_config_explicit_files_with_walkup_tsconfig_ignore_config_synthesizes`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli special_modes_ignore_config_with_no_inputs_follow_no_input_behavior`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-cli`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual repro from #6010 exits 0 and prints `noEmit`, `ignoreConfig`, `files`, and `include`.

## Notes

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli show_config` still fails on
  three unrelated upstream cases: direct path-option normalization,
  `listFilesOnly` parent-config discovery, and inherited selector rendering.
