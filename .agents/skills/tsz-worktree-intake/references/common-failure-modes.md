# TSZ Intake Failure Modes

Use this reference when the checkout is not obviously ready for the requested
task.

## Merged Branch Still Checked Out

Symptom: `gh pr view` shows `state: MERGED`, while `git status --branch` shows a
feature branch with many commits ahead of its remote.

Preferred response:

1. Inspect dirty files.
2. Create a fresh branch from `origin/main`.
3. Carry only dirty files that belong to the new work.
4. Do not force-push or rewrite the merged branch.

## Dirty Workspace

Symptom: `git status --short` shows modified files before the task starts.

Preferred response:

1. Read the diff.
2. Classify each file as requested work, sibling-agent/user work, generated
   output, or unrelated drift.
3. Stage explicitly by path.
4. Mention excluded dirty files in the final or PR notes when they could confuse
   review.

Never revert dirty files you did not create unless the user explicitly asks.

## Missing TypeScript Corpus

Symptom: `scripts/agents/disk-preflight.sh` reports `typescript=missing` or
`present-but-not-populated`.

Preferred response:

- If the task needs conformance, emit, fourslash, or TypeScript sources, link or
  initialize the corpus before debugging.
- In a sibling worktree, prefer `scripts/setup/link-ts-submodule.sh`.
- If the task is docs, scripts, or skill-only, note that corpus setup was not
  needed.

## Low Disk

Symptom: the disk guard reports low space.

Preferred response:

1. Reuse an existing worktree with cache state.
2. Run `scripts/setup/disk-worktree-guard.sh --auto-prune`.
3. Run `scripts/setup/clean.sh --quiet`.
4. Delete only abandoned worktrees whose owner/PR is understood.
5. Use full cleanup only as a deliberate last resort.

## Overlapping PRs

Symptom: open PR titles, issue references, or base/head branches indicate
another agent already owns the same work.

Preferred response:

- Continue that PR if it is assigned to your lane.
- Comment with a signed handoff if you find useful evidence but cannot take it.
- Start a new PR only when the scope is clearly non-overlapping or stacked on
  the dependency branch.
