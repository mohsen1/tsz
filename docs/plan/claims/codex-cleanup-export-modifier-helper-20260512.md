# [WIP] chore(emitter): centralize declaration export modifier checks

- **Date**: 2026-05-11
- **Branch**: `codex/cleanup-export-modifier-helper-20260512`
- **PR**: #5645
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Centralize the repeated declaration-emitter checks that ask whether a syntax node
has an `export` modifier. This is a behavior-preserving cleanup intended to make
the visibility helper easier to scan and reduce copy-paste branching around
declaration kinds.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/helpers/visibility.rs` (~small helper extraction)
- `docs/plan/claims/codex-cleanup-export-modifier-helper-20260512.md`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-emitter` (2254 passed, 9 skipped)
