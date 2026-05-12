# chore(cli): simplify dynamic import argument presence guard

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-driver-resolution-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Simplify the CLI driver's dynamic import discovery guard by replacing a negated `NodeIndex::is_none()` check with the direct positive `is_some()` predicate. This is a behavior-preserving readability cleanup in import specifier collection.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (~1 LOC)

## Verification

- Planned: `cargo fmt --check`
- Planned: `cargo nextest run -p tsz-cli resolution`
- Planned: `cargo clippy -p tsz-cli --all-targets -- -D warnings`
