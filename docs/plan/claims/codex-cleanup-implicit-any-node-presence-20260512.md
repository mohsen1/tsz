# chore(checker): simplify implicit-any node presence checks

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-implicit-any-node-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Replace repeated inverted `NodeIndex::is_none()` checks in checker implicit-any support with direct `is_some()` checks. This keeps the sentinel-style node presence logic easier to scan without changing control flow or diagnostics.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs` (~3 condition cleanups)

## Verification

- Planned: `cargo fmt --check`
- Planned: `cargo nextest run -p tsz-checker implicit_any`
- Planned: `cargo nextest run -p tsz-checker`
