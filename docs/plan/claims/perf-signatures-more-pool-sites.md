# perf(solver): pool 4 more signatures.rs placeholder sites

- **Date**: 2026-05-09
- **Branch**: `perf/signatures-more-pool-sites`
- **PR**: TBD
- **Status**: claim
- **Workstream**: PERFORMANCE_PLAN T3 (visitor allocator reuse)

## Intent

Followup to #4816. Four additional `placeholder_visited =
FxHashSet::default()` sites in `signatures.rs` (lines 158, 183, 503,
956) reuse the existing `with_signatures_visited` helper.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/signatures.rs` (~30 LOC)

## Verification

- `cargo check -p tsz-solver` (clean)
- `cargo nextest run -p tsz-solver --lib` (5713 passed, 7 skipped)
