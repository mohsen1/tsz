# 100% Conformance Strategy

Last updated: 2026-05-07 on `origin/main` at `ffb1937ee7`.

## Target

Reach 100% TypeScript conformance pass rate, publish focused pull requests, merge green work to `main`, and keep local work moving while CI runs. The target is not just fewer failures; each change must reduce the tracked failure set without introducing new diagnostics, fingerprint drift, or unrelated churn.

## Current Baseline

Snapshot files on `origin/main`:

- `scripts/conformance/conformance-snapshot.json`: `12455/12582` passed, `127` failed, `99.0%`.
- `scripts/conformance/conformance-detail.json`: `12455/12581` passed, `126` failed, `5` known failures.

Dashboard from `python3 scripts/conformance/query-conformance.py --dashboard`:

- False positives: `22` tests.
- Fingerprint-only failures: `76` tests.
- Wrong-code failures: `24` tests.
- Close to passing, diff <= 2: `40` tests.
- Big 3 wrong-code problems: `21` tests across `TS2322`, `TS2339`, and `TS2345`.
- Node lane estimate: `5` tests.

Merged after this baseline and expected to move it:

- `#4363` fixed duplicate ES import binding diagnostics. Expected `+1`, merged in `79ba33b93d`.
- `#4366` fixed `import.defer` and JSON import attributes. Expected `+2`, merged in `ffb1937ee7`.

Open active PRs to account for before choosing duplicate work:

- `#4364` `fix(checker): avoid false TS2589 on bounded conditionals`: expected `+5`, full CI running.
- `#4369` `checker: report all duplicate import aliases`: expected `+1`, full CI running.

## Work Selection

Use this order unless a CI failure or main regression blocks merging:

1. Merge green, focused conformance PRs first.
2. Prefer one-extra false positives that flip a test with a narrow suppression or semantic correction.
3. Prefer fingerprint-only clusters when the root cause is a printer/counting rule shared by several tests.
4. Avoid broad semantic rewrites unless a cluster has a clear invariant and a focused regression suite.
5. Do not update snapshots or README stats until the implementation PRs are merged or intentionally batched.

The live high-value commands are:

```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --one-extra
python3 scripts/conformance/query-conformance.py --close 2
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --campaigns
```

## Active Campaigns

### Tier 1: Fingerprint Parity

This is the largest bucket: `76` tests, about `62%` of failures.

Primary targets:

- `TS2322`: `46` fingerprint-only tests.
- `TS2345`: `23` fingerprint-only tests.
- `TS2339`: `8` fingerprint-only tests.
- Diagnostic-count parity for duplicated or missing instances.

Rules:

- Compare expected and actual fingerprint tuples, not only code sets.
- Fix shared formatter/counting rules, not individual string literals.
- Re-run representative neighboring tests for any printer/count rule.

### Tier 2: False Positives

The one-extra queue has `33` tests that can pass by removing one extra code.

Current priority codes:

- `TS2345`: `6` tests.
- `TS2589`: `5` tests, covered by open PR `#4364`.
- `TS2344`: `4` tests.
- `TS2322`: `4` tests.
- `TS2339`: `2` tests.
- `TS7006`: `2` tests.

Rules:

- A false-positive fix must prove the expected diagnostic is genuinely absent in tsc, not merely hidden by a later pass.
- Prefer semantic gating at the diagnostic boundary over post-filtering by filename.
- Include positive tests where the diagnostic must still fire.

### Tier 3: Subsystems

Keep module resolution and parser recovery isolated:

- Module resolution currently has low remaining impact after `#4366`; do not mix resolver work with checker relation work.
- Parser recovery fixes should be measured by TS1xxx cascade reduction and must include malformed syntax regression tests.

## Agent and Worktree Plan

Every agent gets a dedicated worktree and branch from current `origin/main`. Agents must not share write paths.

Current assignments:

- `codex/agent-ts2344-fp`: TS2344 false positives in `inferenceDoesNotAddUndefinedOrNull.ts`, `libdtsFix.ts`, `parserOverloadOnConstants1.ts`.
- `codex/agent-contextual-fp`: contextual TS2322/TS2345 false-positive cluster.
- `codex/agent-type-display-parity`: TS2322/TS2345 fingerprint-only type display cluster.

Local coordination rules:

- Sync `origin/main` before creating or rebasing worktrees.
- Before publishing, rebase on current `origin/main`.
- Do not publish two PRs that edit the same checker hot path unless one is explicitly stacked on the other.
- Clean generated `TypeScript`, `.target`, `target`, and `artifacts` directories from completed worktrees.
- Remove merged/no-diff worktrees quickly.

## Validation Gates

Every implementation PR needs:

- Focused conformance run for each target test.
- Neighbor conformance run for the touched category when cheap.
- Targeted Rust regression tests.
- `cargo fmt` for affected packages or `cargo fmt --all` when broad.
- `git diff --check`.
- Pre-commit hook before commit unless there is a documented resource blocker.

Before merge:

- Full CI green for non-draft PRs.
- Snapshot gate green.
- No `[codex]` or similar prefix in titles or commit messages.
- PR body lists expected conformance impact and validation.

## Whack-a-Mole Controls

For every fix, record:

- The exact failing tests before the change.
- The expected code/fingerprint delta.
- A positive counterexample that still emits the diagnostic.
- At least one neighboring test in the same category.

Reject or rework a patch if it:

- Moves a failure from false-positive to wrong-code without net pass improvement.
- Fixes a single filename through hard-coded special casing.
- Changes snapshot or README stats without matching implementation and conformance evidence.
- Touches broad relation, inference, or type-printer code without focused and neighboring validation.

## Main Sync and Reverts

Main can move while PRs run. The merge loop is:

1. Fetch `origin/main`.
2. Check open PR states without waiting on pending CI.
3. Merge fully green focused PRs.
4. Rebase active local worktrees onto new `origin/main`.
5. Re-run only the focused affected tests after rebase.

If a merged commit reduces pass rate or breaks the conformance goal, identify the responsible commit with focused runs and revert it with a normal repo-style revert commit. Do not revert unrelated user work.

## Disk and Process Hygiene

Current disk after cleanup: about `278Gi` available on `/Users`; `/Users/mohsen/code/tsz-worktrees` is about `11G`, salvage archive about `10M`.

Rules:

- Keep only active PR worktrees, active agent worktrees, and one clean strategy/main worktree.
- Delete merged or no-diff worktrees after confirming status.
- Salvage dirty worktree patches before deletion when the work is not already published.
- Avoid opening long-lived shell sessions unnecessarily.
