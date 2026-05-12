# chore(cli): simplify dynamic import argument presence check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-cli-dynamic-import-presence-20260512`
- **PR**: #5680
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Replace the inverted `NodeIndex::is_none()` guard in CLI dynamic import collection with a direct `is_some()` check. This keeps the import discovery path behavior-preserving while making the sentinel check match the clearer presence style used elsewhere.

## Files Touched

- `crates/tsz-cli/src/driver/resolution.rs` (~1 condition cleanup)

## Verification

- `cargo fmt --check` (passed)
- `cargo nextest run -p tsz-cli resolution_tests` (45 passed, 1560 skipped)
- `cargo nextest run -p tsz-cli` (attempted; local run reported 1507 passed, 84 failed, 14 skipped from unrelated existing driver/tsc-compat expectations)
- CI unit/conformance/fourslash/emit matrix (pending on PR)
