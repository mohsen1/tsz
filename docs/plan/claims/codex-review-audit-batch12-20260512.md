# chore(audit): retire resolved interner/cache/arena review candidates

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close additional stale important threads that are already reflected in current
code/docs.

## Evidence Snapshot

- review comments left on #4993: interner counter-gating commentary no longer
  claims compile-time optimizer elimination; wording now describes the runtime
  gate and predictable skipped branch behavior.
- review comments left on #5063: cross-file query helper docs explicitly call
  out that class-instance cache readers intentionally do **not** apply blanket
  sentinel filtering and require per-call-site policy.
- review comments left on #5095: node-arena overflow behavior has dedicated
  panic-message coverage (`len_u32_overflow_panics_with_expected_message`) and
  no longer depends on unrelated source-file overflow constants.

## Files Touched

- `docs/plan/claims/codex-review-audit-batch12-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
- Spot checks in:
  - `crates/tsz-solver/src/intern/core/interner.rs`
  - `crates/tsz-checker/src/context/cross_file_query.rs`
  - `crates/tsz-parser/src/parser/node_arena.rs`
