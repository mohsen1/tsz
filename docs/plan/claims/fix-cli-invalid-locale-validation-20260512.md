# [WIP] fix(cli): validate invalid locale values

- **Date**: 2026-05-12
- **Branch**: `fix/cli-invalid-locale-validation-20260512`
- **Issue**: #6009
- **Status**: ready
- **Workstream**: 2 (existing CLI bug; user-facing diagnostic parity)

## Intent

Make `tsz --locale <invalid>` match `tsc` by emitting TS6048 and exiting
non-zero instead of silently falling back to the default English locale.

## Files Touched

- `crates/tsz-cli/src/localization/locale.rs`
- CLI startup/driver validation path as needed
- CLI tests for invalid locale handling
- `crates/tsz-checker/src/context/mod.rs` one-line build unbreak for the duplicate lifetime shell re-export present on current main

## Verification

- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli test_is_valid_locale_shape_matches_typescript_rule`
- `env CARGO_INCREMENTAL=0 cargo test -p tsz-cli invalid_locale`
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-cli`
- `env CARGO_INCREMENTAL=0 cargo build --release -p tsz-cli --bin tsz`
- Manual repro from #6009:
  - `tsz --locale does-not-exist index.ts --ignoreConfig --pretty false` exits 1 with TS6048
  - `tsz --locale does-not-exist --pretty false` in a directory with `tsconfig.json` exits 1 with TS6048
  - `tsz --locale en index.ts --ignoreConfig --pretty false --noEmit` exits 0, preserving fallback for well-formed locales
