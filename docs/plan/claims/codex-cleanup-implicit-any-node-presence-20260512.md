# chore(checker): simplify implicit-any node presence checks

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-implicit-any-node-presence-20260512`
- **PR**: #5675
- **Status**: abandoned
- **Workstream**: DRY cleanup

## Intent

Replace repeated inverted `NodeIndex::is_none()` checks in checker implicit-any support with direct `is_some()` checks. This scope was already present on fresh `origin/main`, so this claim was abandoned before implementation.

## Files Touched

- None; abandoned before implementation.

## Verification

- Not run; abandoned before implementation because the intended cleanup already exists on `origin/main`.
