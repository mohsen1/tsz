# chore(binder): simplify stmt presence filter

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-binder-stmt-presence-20260512`
- **PR**: #5667
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Simplify the binder CommonJS indicator scanner's sentinel `NodeIndex`
statement filter from `!idx.is_none()` to `idx.is_some()`. This is a
behavior-preserving readability cleanup in the binder's existing
CommonJS module indicator traversal.

## Files Touched

- `crates/tsz-binder/src/state/core.rs`
- This claim file

## Verification

- `cargo fmt --check` (passed)
- `cargo nextest run -p tsz-binder` (472 passed, 0 skipped)
- pre-commit hook: fmt, clippy for `tsz-binder`, affected direct nextest
  (472 passed, 0 skipped)
