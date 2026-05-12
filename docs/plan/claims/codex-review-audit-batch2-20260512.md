# chore(audit): align claim status and comment precision

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch2-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close remaining important review-audit follow-ups around metadata correctness and technical precision: normalize stale claim status values, remove over-strong optimizer wording in interner perf comments, and rename a misleading JS-emitter regression test that implied declaration-emit coverage.

## Files Touched

- `crates/tsz-solver/src/intern/core/interner.rs`
- `crates/tsz-emitter/src/emitter/expressions/access.rs`
- `docs/plan/claims/fix-declaration-recursive-alias-ts2589-2026-05-10.md`
- `docs/plan/claims/perf-t0-interner-cost-comment-precision-2026-05-10.md`
- `docs/plan/claims/codex-review-audit-batch2-20260512.md`

## Verification

- `cargo test -p tsz-emitter js_emit_comment_positions_around_names_and_property_access -- --nocapture`
- `cargo check -p tsz-solver`
