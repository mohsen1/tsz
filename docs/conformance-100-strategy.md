# 100% Conformance Strategy

Last updated: 2026-05-08 on `origin/main` at `fe5898911d`.

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

## Latest Local Baseline

Latest full local snapshot run after `#4558` and `#4559`:

- `scripts/conformance/conformance-snapshot.json`: `12511/12582` passed, `71` failed, `99.4%`.
- `scripts/conformance/conformance-detail.json`: `12511/12581` passed, `70` failed, `5` known failures.

Dashboard from `python3 scripts/conformance/query-conformance.py --dashboard`:

- Category split: `2` false positives, `6` wrong-code tests, `59` fingerprint-only tests.
- All-missing failures: `0`.
- Top extra codes: `TS2345` in `2` tests, `TS2322` in `2`, plus singleton `TS2349`, `TS1102`, `TS2304`, and `TS2339`.
- Top missing codes are singleton: `TS1127`, `TS1134`, `TS1389`, and `TS2416`.
- Close-to-passing by code-set diff <= 2: `7` code-set tests, with most remaining failures now fingerprint-only.

The behavior queue improved materially during 2026-05-07 and 2026-05-08.
This local snapshot was generated after `origin/main` advanced to `b39d5d3180`
with `#4558` and `#4559` merged. Since that run, `#4560`, `#4563`, and `#4564`
also merged on `main`, so the checked-in README numbers still describe the
latest accepted snapshot artifacts rather than the current behavior frontier.
The generated artifacts were not checked in because the snapshot gate rejects
refreshes that introduce more new failure entries than fixed entries, even when
the total pass count is unchanged. Keep using focused conformance runs for
candidate selection because active behavior PRs can stale this snapshot quickly.
Older notes that cite pre-`#4558` parser/keyof failures, `12451/12582`,
`12470/12582`, `12488/12581`, `12501/12582`, or 99.0-99.3% are stale.

## Active PR Queue

Monitor these PRs without sitting idle for CI:

| PR | Purpose | Action |
| --- | --- | --- |
| #4562 | `fix(checker): preserve inferred prop type equality` | Non-draft behavior PR; CI running on amended architecture-boundary cleanup. Expected to remove the `propTypeValidatorInference.ts` false positive after merge. |
| #4565 | `ci(bench): extend workflow-run debounce` | Non-conformance CI PR; monitor separately and do not count toward diagnostic progress. |
| #4550 | `[WIP] fix(checker): drop hardcoded Comparable<number> diagnostic rewrite (#3057)` | Draft; do not merge. |
| #4517 | `[do not merge] chore(checker-tests): consolidate load_lib_files_for_test variants` | Draft; do not merge. |
| #4428 | `fix(checker): prefer local interface symbols over leaked generic scope` | Draft; leave blocked unless it is rebased and revalidated. |

Recently merged during this cycle and no longer active: `#4430`, `#4433`, `#4434`,
`#4438`, `#4439`, `#4443`, `#4444`, `#4447`, `#4448`, `#4449`, `#4450`,
`#4451`, `#4475`, `#4479`, `#4480`, `#4490`, `#4491`, `#4492`, `#4494`,
`#4495`, `#4496`, `#4497`, `#4498`, `#4499`, `#4500`, `#4501`, `#4488`,
`#4504`, `#4502`, `#4503`, `#4505`, `#4507`, `#4508`, `#4509`, `#4511`,
`#4513`, `#4514`, `#4515`, `#4516`, `#4518`, `#4519`, `#4521`, `#4522`,
`#4523`, `#4520`, `#4525`, `#4526`, `#4527`, `#4528`, `#4531`, `#4532`,
`#4544`, `#4545`, `#4546`, `#4547`, `#4548`, `#4549`, `#4551`, `#4552`,
`#4554`, `#4555`, `#4556`, `#4557`, `#4558`, `#4559`, `#4560`, `#4561`,
`#4563`, and `#4564`. `#4510` and `#4512` also merged and are no longer
active.

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
| Type display parity | `59` fingerprint-only tests | Shared type-printer/display policy for recurring `TS2322`, `TS2345`, `TS2339`, and lib declaration fingerprints. |
| False positives | Snapshot shows `2` tests where tsc expects no diagnostics | `#4560` is merged; `#4562` covers `propTypeValidatorInference.ts` and is waiting on CI. |
| Wrong-code singleton gaps | Snapshot shows `6` tests | `#4564` is merged for the extra `TS1102`; remaining singleton gaps need refreshed current-main confirmation before selection. |
| Parser recovery | `constructorWithIncompleteTypeAnnotation.ts` and `unicodeEscapesInNames02.ts` | Specific TS1xxx parser recovery and unicode escape diagnostic selection. |
| Node/Salsa relation diagnostics | `node` has `4` failures, `salsa` has `2` | Keep module/export, JS, and relation fixes in isolated PRs with focused regression coverage. |

Active local worker assignments from this tranche:

- `codex/conformance-type-display-*` for type display parity; first slice merged as `#4515`.
- `codex/conformance-diagnostic-count-*` for diagnostic count accuracy.
- `codex/parser-syntax-stale-20260508` landed as `#4558`; remaining parser recovery is now `constructorWithIncompleteTypeAnnotation.ts` and `unicodeEscapesInNames02.ts`.
- `codex/big3-relation-*` for a focused Big 3 wrong-code semantic fix.
- `codex/one-extra-diagnostic-*` for a focused one-extra or false-positive diagnostic fix; bundled-lib filtering merged as `#4507`.
- `codex/keyof-indexed-access-20260508` landed as `#4559`; remaining `keyof` failures are fingerprint-only, not the older indexed-write code-set issue.
- `codex/ts2552-temporal-*` for the one-missing TS2552 lane in `temporal.ts`; published as `#4510`.
- `codex/js-salsa-one-extra-*` for remaining JS/Salsa one-extra false positives.
- `codex/inference-contextual-fp-20260508043950` landed as `#4560` for the generic rest callback false positive.
- `codex/delete-invalid-ops-ts1102-20260508` landed as `#4564` for the extra `TS1102` in `deleteOperatorInvalidOperations.ts`.
- `codex/proptype-inference-20260508005442` is published as `#4562` for the `propTypeValidatorInference.ts` false positive.
- `codex/assignment-compat-signature-ts2741-20260508` is checking whether the signature `TS2741` lane is stale after `#4554`.
- `codex/snapshot-refresh-20260508-post4544` owns strategy and README refresh tooling based on current `origin/main`; generated snapshot artifacts should be refreshed in a later PR once the snapshot gate accepts the failure-set delta.

Already published or merged from earlier tranches:

- `fix/generic-functions-context-sensitive` as `#4447`
- `fix/new-target-narrowing-ts2339` as `#4445`
- `fix/type-param-tag-ts2339` as `#4450`
- `codex/fix-3985-unknown-non-strict-errors` as `#4475`
- `codex/delete-operator-ts1102` as `#4500`
- `codex/issue-3985-unknown-nonstrict-ops` as `#4488`
- `grind-iter-20260507-220516` as `#4502`
- `codex/conformance-diagnostic-count-*` as `#4503`
- `fix/scanner-u2028-line-terminators-*` as `#4505`
- `codex/one-extra-diagnostic-*` as `#4507`
- `fix/wasm-program-target-checker-*` as `#4508`
- `fix/wasm-lsp-server-js-shape-*` as `#4509`
- `codex/indexed-write-followup-*` as `#4511`
- `fix/server-geterr-emits-events-*` as `#4514`
- `codex/conformance-type-display-*` as `#4515`
- `fix/cli-output-only-tsconfig-*` as `#4516`
- `fix/scanner-u2028-line-terminators-*` as `#4512`
- `fix/checker-temporal-ts2552-*` as `#4510`
- `fix/server-encoded-syntactic-classifications-*` as `#4521`
- `fix/cli-no-check-js-syntactic-diagnostics-*` as `#4520`
- `fix/checker-template-index-properties-*` as `#4525`
- `fix/emitter-keyof-generic-parens-*` as `#4527`

## Campaigns

### Tier 1: Fingerprint Parity

Fingerprint parity is most of the remaining work: `59` tests and most of the
remaining failures. The recurring families are assignment/call/property-access
display parity, lib declaration fingerprints, and parser recovery fingerprints
where the code set now matches.

Root-cause campaigns from `--campaigns`:

- Type display parity: estimated `30` tests from the checked-in dashboard, with the campaign query still flagging broader message-format impact.
- Diagnostic count accuracy: estimated `12` tests from the checked-in dashboard, with broader count-rule impact in the campaign query.

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
