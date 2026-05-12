# chore(parser): simplify JSX close-tag presence check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-jsx-close-tag-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Replace the inverted `NodeIndex::is_none()` check in JSX close-tag mismatch handling with a direct `is_some()` check. The parser logic remains behavior-preserving while matching the clearer presence-check style used elsewhere.

## Files Touched

- `crates/tsz-parser/src/parser/state_types_jsx.rs` (~1 condition cleanup)

## Verification

- Planned: `cargo fmt --check`
- Planned: `cargo nextest run -p tsz-parser jsx`
- Planned: `cargo nextest run -p tsz-parser`
