# fix(checker): read merged variance declarations cross-file

- **Date**: 2026-05-12
- **Branch**: `fix/variance-merged-decl-arenas-20260512`
- **PR**: #5726
- **Status**: ready
- **Workstream**: conformance

## Intent

Follow up on review feedback from PR #5714. Merged interface variance annotations must read every declaration from its owning arena, including cross-file declarations, so merged `in`/`out` modifiers contribute to generic assignability consistently.

## Files Touched

- `crates/tsz-checker/src/context/resolver.rs`

## Verification

- `cargo fmt --check -p tsz-checker` (passed)
- `cargo nextest run -p tsz-checker variance` (22 passed, 7498 skipped)
- `cargo clippy -p tsz-checker --all-targets -- -D warnings` (passed)
