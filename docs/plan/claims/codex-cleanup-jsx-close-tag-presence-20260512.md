# chore(parser): simplify JSX close-tag presence check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-jsx-close-tag-presence-20260512`
- **PR**: #5676
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Replace the inverted `NodeIndex::is_none()` check in JSX close-tag mismatch handling with a direct `is_some()` check. The parser logic remains behavior-preserving while matching the clearer presence-check style used elsewhere.

## Files Touched

- `crates/tsz-parser/src/parser/state_types_jsx.rs` (~1 condition cleanup)

## Verification

- `cargo fmt --check` (passed)
- `cargo nextest run -p tsz-parser jsx` (44 passed, 828 skipped)
- `cargo nextest run -p tsz-parser` (871 passed, 1 skipped)
- `cargo clippy -p tsz-parser --all-targets -- -D warnings` (passed)
- `scripts/safe-run.sh cargo nextest run` (attempted; local linker failed with `No space left on device` while building unrelated checker/wasm test binaries)
