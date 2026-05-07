# 100% Conformance Strategy

Last updated: 2026-05-07 on `origin/main` at `9b2535e8fc`.

## Target

Reach 100% TypeScript conformance pass rate, publish focused pull requests, merge green work to `main`, and keep local work moving while CI runs. The target is not just fewer failures; each change must reduce the tracked failure set without introducing new diagnostics, fingerprint drift, or unrelated churn.

## Current Baseline

Snapshot files on `origin/main`:

- `scripts/conformance/conformance-snapshot.json`: `12470/12582` passed, `112` failed, `99.1%`.
- `scripts/conformance/conformance-detail.json`: `12470/12581` passed, `111` failed, `5` known failures.

Dashboard from `python3 scripts/conformance/query-conformance.py --dashboard`:

- False positives: `14` tests.
- Fingerprint-only failures: `71` tests.
- Wrong-code failures: `22` tests.
- Close to passing, diff <= 2: `30` tests.
- Big 3 wrong-code problems: `20` tests across `TS2322`, `TS2339`, and `TS2345`.
- Node lane estimate: `5` tests.

Recently merged reductions already in this baseline:

- `#4408` simplified the slow TypeBox structural display fixture so CI can finish.
- `#4410` fixed `in` operator diagnostics after type-environment prewarm.
- `#4412` fixed DOM indexed-access TS2344 variance drift.
- `#4413` fixed unconstrained generic JSX attribute TS2322 emission.

Open active PRs to account for before choosing duplicate work:

- `#4415` `fix(solver): exclude pure-literal unions from generic-display origin override`: expected `+19`, auto-merge enabled, CI running.
- `#4401` `test(checker): lock jsx excess prop assignability`: auto-merge enabled, CI running.
- `#4392` `chore(conformance): prune stale suppression debt`: auto-merge enabled, CI running.
- `#4414` `fix(bench): include full nextjs project shard`: not a conformance fix, auto-merge enabled.

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

This is the largest bucket: `71` tests, about `64%` of detail failures.

Primary targets:

- `TS2322`: `45` fingerprint-only tests.
- `TS2345`: `24` fingerprint-only tests.
- `TS2339`: `8` fingerprint-only tests.
- `TS2564`: `7` fingerprint-only tests.
- Diagnostic-count parity for duplicated or missing instances.

Rules:

- Compare expected and actual fingerprint tuples, not only code sets.
- Fix shared formatter/counting rules, not individual string literals.
- Re-run representative neighboring tests for any printer/count rule.

### Tier 2: False Positives

The one-extra queue has `21` tests that can pass by removing one extra code.

Current priority codes:

- `TS2345`: `5` tests.
- `TS2344`: `4` tests.
- `TS2322`: `4` tests.
- `TS2339`: `2` tests.
- `TS2638`: `2` tests.
- `TS7006`: `2` tests.
- `TS2741`: `2` tests.

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

- `fix/parser-recovery-conformance`: parser/binder recovery for `plainJSBinderErrors.ts`, then `reachabilityChecksNoCrash1.ts` and `unicodeEscapesInNames02.ts` if same root cause.
- `fix/module-preserve4-conformance`: `modulePreserve4.ts` extra `TS1192`.
- `fix/contextual-ts2345-conformance`: contextual/generic extra `TS2345` in `observableInferenceCanBeMade.ts`, `promiseTry.ts`, `templateLiteralTypes6.ts`, and `unionTypeReduction2.ts`.
- `fix/variance-ts2344-conformance`: missing `TS2344` in `coAndContraVariantInferences2.ts`, `coAndContraVariantInferences3.ts`, and `coAndContraVariantInferences4.ts`.
- Existing `fix/iterator-diagnostic-conformance`: iterator helper `TS2339`/`TS7006` cluster; avoid duplicate local work until that branch reports.

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

Current disk check: about `79Gi` available on `/Users` as of 2026-05-07. This is enough for active work, but old worktrees and generated targets need continuous pruning.

Rules:

- Keep only active PR worktrees, active agent worktrees, and one clean strategy/main worktree.
- Delete merged or no-diff worktrees after confirming status.
- Salvage dirty worktree patches before deletion when the work is not already published.
- Avoid opening long-lived shell sessions unnecessarily.
