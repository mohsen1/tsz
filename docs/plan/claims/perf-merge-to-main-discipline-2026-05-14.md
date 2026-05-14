# Claim: Enforce merge-to-main discipline in the performance plan

Date: 2026-05-14

## Claim

Add explicit process requirements to `docs/plan/PERFORMANCE_PLAN.md` so perf
work is considered complete only after merge to `main`, with required
rebase/rerun/cancel behavior while `main` moves.

## Evidence

- `docs/plan/PERFORMANCE_PLAN.md`
  - Adds **Section 17: Merge-To-Main Discipline (Required)**.
  - Requires dedicated worktrees on `origin/main`.
  - Requires checking `HEAD...origin/main` before push/rerun/claims.
  - Requires immediate rebase when `behind > 0`.
  - Requires stale-run cancellation and periodic drift monitoring.
  - Requires post-merge lane cleanup to control disk growth.

## Validation

- Documentation-only change; no runtime or test-surface impact.
