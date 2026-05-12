# chore(solver): make intern test threshold export cfg-test only

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-solver-intern-test-export-20260512`
- **PR**: #5700
- **Status**: ready
- **Workstream**: 8.4 (lint cleanup)

## Intent

Remove an `#[allow(unused_imports)]` from the solver interner module by
scoping the `PROPERTY_MAP_THRESHOLD` re-export to tests, where the
path-included interner tests actually read it. This keeps production intern
module and core-module exports aligned with their consumers without hiding
unused-import feedback.

## Files Touched

- `crates/tsz-solver/src/intern/mod.rs`
- `crates/tsz-solver/src/intern/core/mod.rs`
- `docs/plan/claims/codex-cleanup-solver-intern-test-export-20260512.md`

## Verification

- `cargo fmt -p tsz-solver`
- `cargo check -p tsz-solver`
- `cargo test -p tsz-solver intern::tests::test_interner_object_property_lookup_cache`
- `cargo clippy -p tsz-solver --all-targets -- -D warnings`
