# chore(checker): simplify heritage parent traversal check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-heritage-presence-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Simplify the checker heritage-support parent traversal sentinel check from
`!current.is_none()` to `current.is_some()`. This is a behavior-preserving
readability cleanup in the helper that collects enclosing interface type
parameter names.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/heritage_support.rs`
- This claim file

## Verification

- Planned: `cargo fmt --check`
- Planned: targeted `cargo nextest run -p tsz-checker heritage_support`
- Planned: pre-commit checks for the focused checker cleanup
