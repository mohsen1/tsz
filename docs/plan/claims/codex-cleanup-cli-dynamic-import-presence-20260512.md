# chore(cli): simplify dynamic import argument presence check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-cli-dynamic-import-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Replace the inverted `NodeIndex::is_none()` guard in CLI dynamic import collection with a direct `is_some()` check. This keeps the import discovery path behavior-preserving while making the sentinel check match the clearer presence style used elsewhere.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (~1 condition cleanup)

## Verification

- Planned: `cargo fmt --check`
- Planned: `cargo nextest run -p tsz-cli`
- Planned: CI unit/conformance/fourslash/emit matrix
