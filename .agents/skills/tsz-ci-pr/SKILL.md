---
name: tsz-ci-pr
description: Drive TSZ GitHub PRs through review, CI, and the merge queue. Use when checking PR status, debugging failing GitHub Actions, deciding whether a PR is WIP, marking draft PRs ready, enqueueing or verifying the queue, landing a PR, or interpreting TSZ CI gates.
---

# TSZ CI And PR

Use for PR checks, CI triage, ready state, queue evidence, and landing. Use
`tsz-pr-coordination` for PR body/label contract.

## Rules

- Use `gh`; sign substantive comments with a provenance line (e.g.
  `Machine: studio` or `Provenance: studio / claude-code / claude-opus-4-8 /
  high`).
- The PR author lands their own PR; never idle-wait on CI between pushes.
- Never merge draft/WIP PRs (`draft`, `WIP`, `[WIP]`, or body/branch says WIP).
- PR-head CI gates the active rewrite with `clippy`, `arch-size`, and `CI Summary`.
  Full conformance, emit, and fourslash jobs run nightly or by manual dispatch
  as observations until roadmap families graduate into the monotonic floor.
- Ready PRs land through GitHub's native merge queue.
- If asked to land, verify `state: MERGED`; an armed queue request alone is not enough.

## Inspect

```bash
gh pr view <pr> --json state,isDraft,mergeStateStatus,mergeable,autoMergeRequest,mergedAt,labels,title,url,headRefName,headRefOid
gh pr checks <pr> --json name,state,bucket,link,completedAt
gh run view <run-id> --log-failed
gh run view <run-id> --job <job-id> --log
```

Use `gh pr checks <pr> --watch --interval 20` only when the result is needed
now.

## Landing

1. Confirm not WIP and head SHA matches inspected checks.
2. Fix failures in-PR; comment with root cause and verification.
3. Mark ready only after implementation and verification:
   `gh pr ready <pr>`.
4. Queue with GitHub's native merge queue only when exact-head PR-head checks
   such as `CI Summary` pass and policy permits:
   `gh pr merge <pr> --match-head-commit <sha>`.
   Do not pass `--auto`; with branch protection and passing checks, current
   `gh` adds the PR to the native merge queue directly.
5. Inspect `merge_group` CI runs if queue validation stalls.
6. Verify final merge:
   `gh pr view <pr> --json state,mergedAt,mergedBy,url`.

## Failure Hints

- `CI Summary`: umbrella PR-head status.
- `merge_group` CI: native queued-merge summary validation.
- `clippy`: formatting, active-workspace check, Clippy, rewrite nextest, and
  CI contract checks share this retained required-check name.
- `arch-size`: replacement-boundary, retired-source, Sound Mode, and file-size
  guards.
- `*-observation-*`: nightly/manual infrastructure failures are actionable;
  behavior mismatches are retained in artifacts and continued inside the job.

Comment shape: provenance line, root cause, files changed, verification/CI
URL, remaining risk.
