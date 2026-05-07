# 100% Conformance Strategy

Last updated: 2026-05-07 on `origin/main` at `61b566fd3a`.

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

Current checked-in snapshot on `origin/main`:

- `scripts/conformance/conformance-snapshot.json`: `12488/12581` passed, `93` failed, `99.3%`.
- `scripts/conformance/conformance-detail.json`: `12488/12581` passed, `93` failed, `4` known failures.

Dashboard from `python3 scripts/conformance/query-conformance.py --dashboard`:

- Big 3 wrong-code problems: `20` tests across `TS2322`, `TS2339`, and `TS2345`.
- Other tracked parser/excess-property codes: `1` test.
- Likely crashes: `0`.
- Node lane estimate: `5`.
- Fingerprint-only failures: `59` tests, about `66%` of remaining failures.
- False positives where tsc expects no diagnostics: `13` tests.
- Diff <= 2: `24` tests.

The main queue improved materially during 2026-05-07. Older notes that cite
`12451/12582`, `12470/12582`, or 99.0-99.1% are stale.

## Active PR Queue

Monitor these PRs without sitting idle for CI:

| PR | Purpose | Action |
| --- | --- | --- |
| #4436 | `fix(cli): account for rootDir in default tsbuildinfo path` | Not auto-merge enabled at last check; resolve dirty merge state before queueing. |
| #4435 | `fix(server): emit fixMissingFunctionDeclaration for plain unresolved calls` | Auto-merge enabled; CI running. |
| #4434 | `fix(server): implement-interface fix supports method signatures (#3938)` | Auto-merge enabled; CI running. |
| #4433 | `perf(checker): precompile ambient module globs` | Auto-merge enabled; unit/lint passed, conformance/fourslash running. |
| #4430 | `fix(server): honor generateReturnInDocTemplate user preference` | Auto-merge enabled; mostly green, one fourslash shard still running at last check. |
| #4428 | `fix(checker): prefer local interface symbols over leaked generic scope` | Auto-merge enabled; force-pushed after rebase, wait for new checks to appear. |
| #4425 | `fix(server): match TODO comments inside template substitutions` | Auto-merge enabled; previous failure was fourslash shard 3 plus aggregate, rerun already active. |

Do not duplicate recently merged conformance work from `#4432`, `#4419`, `#4417`,
or the assignment-compat work in `#4428`.

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

Best one-extra targets on current `main`, excluding work already covered by open PRs:

| Test | Extra code | Suggested lane |
| --- | --- | --- |
| `complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound.ts` | `TS2339` | Indexed access / `keyof never` property lookup. |
| `genericFunctionsNotContextSensitive.ts` | `TS7006` | Context-sensitive function parameter inference. |
| `propTypeValidatorInference.ts` | `TS2322` | Contextual return / object literal assignability. |
| `controlFlowAssignmentPatternOrder.ts` | `TS2322` | Control-flow assignment pattern narrowing. |
| `newTargetNarrowing.ts` | `TS2339` | `new.target` narrowing and function expando properties. |
| `parserOverloadOnConstants1.ts` | `TS2430` | Parser/binder treatment of overload-like constant declarations. |
| `typeFromParamTagForFunction.ts` | `TS2339` | JS/Salsa `@param` function shape. |

Active local worker assignments from this tranche:

- `fix/generic-functions-context-sensitive`
- `fix/new-target-narrowing-ts2339`
- `fix/type-param-tag-ts2339`
- `fix/prop-type-validator-ts2322`

## Campaigns

### Tier 1: Fingerprint Parity

Fingerprint parity is most of the remaining work: `59` tests and about two thirds of
failures. The main sub-buckets are:

- `TS2322`: `36` tests.
- `TS2345`: `19` tests.
- `TS2339`: `9` tests.
- `TS2564`: `3` tests.
- `TS2454`: `1` test.

Root-cause campaigns from `--campaigns`:

- Type display parity: estimated `44` tests.
- Diagnostic count accuracy: estimated `43` tests.

Rules:

- Compare complete fingerprint tuples, not only diagnostic codes.
- Fix shared type-printer or diagnostic-counting rules.
- Include representative neighbor tests for any display or count rule.

### Tier 2: Big 3 Relation Errors

The Big 3 queue is still high-value:

- `TS2322`: `7` tests.
- `TS2339`: `7` tests.
- `TS2345`: `6` tests.

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
