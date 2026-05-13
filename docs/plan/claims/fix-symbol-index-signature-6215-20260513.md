# fix: symbol index signature lookup accepts symbol keys

Status: claim
Issue: #6215
Branch: fix-symbol-index-signature-6215-20260513

## Scope
- Investigate and fix the TS7053/TS2322 false positive for indexing a symbol index signature with a symbol/unique symbol key.
- Add focused checker regression coverage for the reported reproduction.

## Coordination
- Created after checking open PRs/issues, active claims, remote branches, worktrees, local status, and disk space on 2026-05-13.
- Avoids open PR #6212 performance cache files and #6217 checker-test helper cleanup unless root cause requires nearby checker paths.

## Verification
- Pending.
