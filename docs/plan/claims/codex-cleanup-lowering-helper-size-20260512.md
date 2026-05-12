# chore(emitter): split lowering name helpers

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-lowering-helper-size-20260512`
- **Base**: `origin/main`
- **Issue**: n/a
- **PR**: tbd
- **Status**: claim
- **Labels**: `DRY`, `emitter`

## Intent

Move focused lowering name/capture helpers out of the oversized
`lowering/helpers.rs` module so the emitter file-size ratchet returns to its
checked-in baseline without a ratchet bump.

## Scope

- `crates/tsz-emitter/src/lowering/helpers.rs`
- `crates/tsz-emitter/src/lowering/name_helpers.rs`
- `crates/tsz-emitter/src/lowering/mod.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-solver --lib -E 'test(solver_file_size_ceiling_tests::test_emitter_file_size_ceiling)' --no-fail-fast`
- `cargo nextest run -p tsz-emitter --lib -E 'test(lowering_helpers)' --no-fail-fast`
