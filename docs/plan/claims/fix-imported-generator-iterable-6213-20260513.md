# fix: imported Generator remains iterable across files

Status: claim
Issue: #6213
Branch: fix-imported-generator-iterable-6213-20260513

## Scope
- Investigate and fix the TS2488 false positive for `for-of` over an imported function returning `Generator<T>`.
- Add focused multi-file checker regression coverage for the issue reproduction.

## Coordination
- Created after checking open PRs/issues, active claims, remote branches, worktrees, local status, and current main on 2026-05-13.
- Avoids #6212 performance-cache files, #6217 checker-test cleanup, and #6218 symbol-index regression files unless root cause requires shared infrastructure.

## Verification
- Pending.
