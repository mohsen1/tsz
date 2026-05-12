# chore(solver-tests): share subtype cache repeat assertions

- **Date**: 2026-05-12
- **Branch**: `dry-solver-subtype-cache-test-helper-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY audit Phase 2 / `tsz-solver` test fixture cleanup

## Intent

`subtype_cache_tests.rs` repeats the same cache-hit contract in several tests:
run a subtype relation, capture the subtype cache entry count, run the same
relation again, and assert the entry count stays stable. This PR extracts that
assertion into a test-local helper and migrates the representative cache-hit and
negative-cache cases onto it.

## Files Touched

- `crates/tsz-solver/tests/subtype_cache_tests.rs` (~40 LOC change)
- `docs/plan/claims/dry-solver-subtype-cache-test-helper-20260512.md`

## Verification

- Pending
