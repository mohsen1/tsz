# 100% Conformance Strategy

Last updated: 2026-05-07 on `origin/main` at `c5a480fc2f`.

## Target

Reach and hold 100% TypeScript diagnostic conformance on `main`.

The goal is complete only when all of these are true on the same merged main commit:

- `scripts/conformance/conformance-snapshot.json` reports zero failures.
- `scripts/conformance/conformance-detail.json` has no remaining unsuppressed failure entries.
- README conformance stats are refreshed from the same snapshot.
- CI conformance shards and aggregate are green.
- No open behavior PR is still required to explain the snapshot.

Fourslash and emit remain merge gates, but they are not substitutes for diagnostic
conformance.

## Current Baseline

Current checked-in snapshot on `origin/main` after `#4475`:

- `scripts/conformance/conformance-snapshot.json`: `12504/12582` passed, `78` failed, `99.4%`.
- `scripts/conformance/conformance-detail.json`: `12504/12581` passed, `77` failed, `5` known failures.

Dashboard from `python3 scripts/conformance/query-conformance.py --dashboard`:

- Big 3 wrong-code problems: `10` tests across `TS2322`, `TS2339`, and `TS2345`.
- Other tracked parser/excess-property codes: `1` test.
- Likely crashes: `0`.
- Node lane estimate: `5`.
- Fingerprint-only failures: `55` tests, about `74%` of remaining failures.
- False positives where tsc expects no diagnostics: `7` tests.
- Diff <= 2: `13` tests.

The behavior queue improved materially during 2026-05-07. The checked-in snapshot
has not yet been refreshed for `#4479`, `#4480`, `#4494`, `#4496`, `#4497`,
`#4498`, `#4499`, or `#4500`, so use focused conformance runs for current-main
candidate selection until the next snapshot/README refresh PR lands. Older notes
that cite `12451/12582`, `12470/12582`, `12488/12581`, or 99.0-99.3% are stale.

## Active PR Queue

Monitor these PRs without sitting idle for CI:

| PR | Purpose | Action |
| --- | --- | --- |
| #4488 | `fix(checker): preserve non-strict unknown operation errors` | Auto-merge enabled. Branch was cleaned and rebased to `c528cfc112` after a contaminated force-push; CI is running. |
| #4428 | `fix(checker): prefer local interface symbols over leaked generic scope` | Draft and not auto-merge enabled because the last version had broad unit/conformance regressions. |

Recently merged during this cycle and no longer active: `#4430`, `#4433`, `#4434`,
`#4438`, `#4439`, `#4443`, `#4444`, `#4447`, `#4448`, `#4449`, `#4450`,
`#4451`, `#4475`, `#4479`, `#4480`, `#4490`, `#4491`, `#4492`, `#4494`,
`#4495`, `#4496`, `#4497`, `#4498`, `#4499`, and `#4500`.

## Work Selection

Use this order unless a red PR requires a focused CI fix:

1. Merge green, focused PRs that already have auto-merge enabled.
2. Pick one-extra false positives that can flip an entire test with a narrow semantic fix.
3. Pick fingerprint-only clusters when one printer/counting rule explains multiple tests.
4. Keep parser recovery, module resolution, checker relation, and server/fourslash work in separate PRs.
5. Refresh snapshots and README stats only after behavior PRs are merged or intentionally batched.

High-signal query commands:

```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --close 2 --top 60
python3 scripts/conformance/query-conformance.py --one-extra --top 60
python3 scripts/conformance/query-conformance.py --fingerprint-only --top 60
```

## Current Next Tranche

Best remaining targets on current `main`, excluding work already covered by open PRs:

| Target | Current impact | Suggested lane |
| --- | --- | --- |
| Type display parity | About `27` tests | Shared type-printer/display policy for `TS2322`, `TS2345`, and `TS2339` fingerprint-only failures. |
| Diagnostic count accuracy | About `11` tests | Remove duplicate diagnostics or add missing instances where tsc emits the same code set. |
| Big 3 wrong-code problems | `10` tests | Relation/property/call diagnostic selection. |
| Parser recovery | `3` tests | Specific TS1xxx parser recovery selection. |

Active local worker assignments from this tranche:

- `codex/conformance-type-display-*` for type display parity.
- `codex/conformance-diagnostic-count-*` for diagnostic count accuracy.
- `codex/conformance-parser-recovery-*` for parser recovery.
- `codex/big3-relation-*` for a focused Big 3 wrong-code semantic fix.
- `codex/one-extra-diagnostic-*` for a focused one-extra or false-positive diagnostic fix.
- `codex/issue-3985-unknown-nonstrict-ops` for #4488, now clean at `c528cfc112`.

Already published or merged from earlier tranches:

- `fix/generic-functions-context-sensitive` as `#4447`
- `fix/new-target-narrowing-ts2339` as `#4445`
- `fix/type-param-tag-ts2339` as `#4450`
- `codex/fix-3985-unknown-non-strict-errors` as `#4475`
- `codex/delete-operator-ts1102` as `#4500`

## Campaigns

### Tier 1: Fingerprint Parity

Fingerprint parity is most of the remaining work: `55` tests and about three quarters of
failures. The main sub-buckets are:

- `TS2322`: `34` tests.
- `TS2345`: `17` tests.
- `TS2339`: `9` tests.
- `TS2564`: `3` tests.
- `TS2454`: `1` test.

Root-cause campaigns from `--campaigns`:

- Type display parity: estimated `27` tests.
- Diagnostic count accuracy: estimated `11` tests.

Rules:

- Compare complete fingerprint tuples, not only diagnostic codes.
- Fix shared type-printer or diagnostic-counting rules.
- Include representative neighbor tests for any display or count rule.

### Tier 2: Big 3 Relation Errors

The Big 3 queue is smaller but still high-value:

- `TS2322`: `5` tests.
- `TS2339`: `2` tests.
- `TS2345`: `3` tests.

Rules:

- Fix the semantic boundary that decides the diagnostic, not a filename-specific post-filter.
- Add a counterexample proving the diagnostic still fires when tsc emits it.
- Avoid stacking multiple broad relation changes unless the later PR is explicitly based on the earlier one.

### Tier 3: Parser, JS/Salsa, Node, And Server

Parser recovery currently has low diagnostic-conformance count but high regression risk.
JS/Salsa and server/fourslash fixes should stay in their own lanes because they often
exercise different harnesses and CI shards.

Rules:

- Parser PRs need parser unit tests plus focused conformance.
- JS/Salsa PRs need focused JS checker tests or harness coverage plus the specific conformance filter.
- Server PRs need focused fourslash/server tests and must watch shard-level CI failures.

## Validation Gates

Every behavior PR needs:

- Focused conformance run for each targeted test.
- Targeted Rust regression tests around the changed code path.
- Neighbor tests for the same relation, inference, parser, or display family when cheap.
- `cargo fmt --check`.
- `git diff --check`.
- A PR body with root cause, conformance impact, and validation.

Before merge:

- Full CI green for non-draft PRs.
- Auto-merge enabled when the branch is otherwise ready.
- No `[codex]` or similar prefix in PR titles or commit messages.
- No unrelated snapshot or README stat churn in behavior PRs.

## Snapshot And README Refresh

Snapshot/readme refresh PRs are separate unless the user explicitly asks for a combined
batch.

Use a clean, current worktree:

```bash
scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot
python3 scripts/refresh-readme.py --write
git diff -- scripts/conformance/conformance-snapshot.json scripts/conformance/conformance-detail.json scripts/conformance/conformance-baseline.txt README.md
```

The PR body must include the exact pass/total/fail numbers and the `origin/main` commit
used for the refresh.

## Worktree, Agent, And Disk Discipline

- Start every independent fix from fresh `origin/main` in a dedicated worktree.
- Agents must not share write paths.
- Main checkout may be dirty; do not overwrite unrelated user changes there.
- Before deleting a worktree, inspect `git status --short --branch`.
- Salvage useful dirty patches before deletion.
- Remove generated `TypeScript`, `.target`, `.target-bench`, `target`, `node_modules`,
  and artifact directories from inactive worktrees.
- Avoid long-lived shell sessions; prefer short commands and close completed agent handles.

## Main Sync And Reverts

Loop:

1. Fetch `origin/main`.
2. Check open PR states and failed checks.
3. Rebase active local worktrees that are about to publish.
4. Merge or enable auto-merge for green focused PRs.
5. Select the next target from current-main data, not stale local snapshots.

If a merged commit reduces pass rate or breaks the conformance path, identify the
responsible commit with focused runs and revert it with a normal repo-style revert
commit. Do not revert unrelated user work.
