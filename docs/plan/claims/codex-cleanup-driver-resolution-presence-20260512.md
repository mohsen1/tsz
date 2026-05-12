# chore(cli): simplify dynamic import argument presence guard

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-driver-resolution-presence-20260512`
- **PR**: #5702
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Simplify the CLI driver's dynamic import discovery guard by replacing a negated `NodeIndex::is_none()` check with the direct positive `is_some()` predicate. This is a behavior-preserving readability cleanup in import specifier collection.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (~1 LOC)

## Verification

- `cargo fmt --check` (passed)
- `cargo nextest run -p tsz-cli dynamic_import` (5 passed, 1600 skipped)
- `cargo clippy -p tsz-cli --all-targets -- -D warnings` (passed)
- `cargo nextest run -p tsz-cli` (1506 passed, 85 failed, 14 skipped; local baseline failures in TS5011 compile fixtures, `es2025.iterator` lib support, showConfig path rendering, help-all parity, and existing declaration emit cases)
