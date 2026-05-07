# Conformance 100% Strategy

Last updated: 2026-05-07
Baseline: `origin/main` at `6b78c057c6`

## Goal

Reach and hold 100% diagnostic conformance against the TypeScript conformance suite.

The measurable gate is `scripts/conformance/conformance-snapshot.json` reporting:

- `total_tests == passed`
- `failed == 0`
- README conformance stats refreshed from that snapshot
- CI conformance shards and aggregate passing on the merged main commit

Fourslash and emit are tracked separately. They are useful guardrails, but they are
not a substitute for diagnostic conformance.

## Current State

Current `origin/main` snapshot:

- Total: 12,582
- Passed: 12,451
- Failed: 131
- Pass rate: 99.0%

Current `scripts/conformance/conformance-detail.json` exposes 107 failure entries.
The difference from the 131 snapshot failures is expected while detail output groups
some failures by source file and known-failure metadata.

Failure categories in detail output:

| Category | Count | Meaning |
| --- | ---: | --- |
| fingerprint-only | 71 | Codes match; span, file, message, or fingerprint differs |
| false-positive | 14 | tsz reports diagnostics where tsc reports none |
| only-extra | 11 | tsz reports all expected codes plus extras |
| only-missing | 8 | tsz misses one or more expected codes |
| wrong-code | 3 | tsz both misses expected codes and reports extras |

High-frequency extra codes:

| Code | Count | Initial focus |
| --- | ---: | --- |
| TS2345 | 7 | generic/contextual inference false positives |
| TS2339 | 6 | indexed access, new.target, JS inheritance shape |
| TS2322 | 6 | assignability false positives and contextual return types |
| TS7006 | 4 | context-sensitive function parameter inference |
| TS2638 | 2 | `in` operator RHS routing |

High-frequency missing codes:

| Code | Count | Initial focus |
| --- | ---: | --- |
| TS2344 | 3 | variance/inference constraints |
| parser syntax codes | 10+ total | parser recovery and diagnostic recovery positions |

## Workstreams

### 1. Fingerprint-Only Cleanup

Owner pattern: one cluster per PR.

Target failures where expected and actual codes match. These are usually lower-risk
because the semantic decision is already correct; the fix should adjust diagnostic
span, related source, display text, or recovery position.

Current examples:

- `arrowFunctionsMissingTokens.ts`
- `declarationEmitObjectAssignedDefaultExport.ts`
- `excessPropertyCheckWithMultipleDiscriminants.ts`
- `fixTypeParameterInSignatureWithRestParameters.ts`
- `genericRestArgs.ts`
- `ignoredJsxAttributes.tsx`
- `inKeywordAndUnknown.ts`

Acceptance for each PR:

- Focused verbose conformance filter passes for every targeted file.
- A local unit/regression test locks the formatting/span behavior when feasible.
- Snapshot is not refreshed in the behavior PR unless the PR is explicitly a snapshot
  refresh.

### 2. Generic and Contextual False Positives

Owner pattern: checker/solver code PRs, with focused Rust tests first.

Targets:

- `genericCallInferenceUsingThisTypeNoInvalidCacheReuseAfterMappedTypeApplication1.ts`
- `genericFunctionsNotContextSensitive.ts`
- `inferenceContextualReturnTypeUnion3.ts`
- `observableInferenceCanBeMade.ts`
- `promiseTry.ts`
- `propTypeValidatorInference.ts`
- `unwitnessedTypeParameterVariance.ts`

Primary risk is whack-a-mole in assignability. Fixes must prove they do not silence
legitimate diagnostics by adding both positive and negative Rust tests near the changed
checker/solver path.

Acceptance for each PR:

- Focused Rust tests for the code path pass.
- Focused conformance filters pass.
- Existing nearby tests for the changed relation/inference path pass.

### 3. Parser and Syntax Recovery

Owner pattern: parser/binder PRs only; do not mix semantic checker changes.

Targets:

- `constructorWithIncompleteTypeAnnotation.ts`
- `reachabilityChecksNoCrash1.ts`
- `unicodeEscapesInNames02.ts`
- `plainJSBinderErrors.ts`
- `arrowFunctionsMissingTokens.ts`

Most failures here are missing or wrong syntax diagnostics after recovery. The strategy
is to inspect tsc and tsz verbose output, then fix the narrow recovery path that chooses
the code/span. Avoid broad parser recovery rewrites unless a failing cluster proves the
same root cause.

Acceptance for each PR:

- Parser unit tests for the malformed syntax shape.
- Focused conformance filters pass.
- No new parser snapshot churn outside targeted cases.

### 4. Indexed Access, `in`, and Object Shape Extras

Owner pattern: checker/solver PRs; one diagnostic family per branch.

Targets:

- `conditionalTypeDoesntSpinForever.ts` extra TS2638
- `inDoesNotOperateOnPrimitiveTypes.ts` extra TS2638
- `inKeywordTypeguard.ts` missing TS18046/TS2322/TS2638
- `complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound.ts` extra TS2339
- `keyofAndIndexedAccess2.ts` extra TS7006
- `indexSignatures1.ts` missing TS2374/TS2413 and extra TS2339

Acceptance for each PR:

- Tests cover both the fixed failing case and a still-erroring counterexample.
- Focused conformance filters pass.
- Run the nearest existing checker/solver test file for regressions.

## PR and CI Discipline

- Work in separate git worktrees from fresh `origin/main`.
- Keep PRs small and independently mergeable.
- Do not include `[codex]` or similar tags in commit messages or PR titles.
- Do not wait idly for CI. After pushing one PR, immediately move to the next
  independent workstream.
- If CI fails, inspect the failing log once there is a completed failure, fix locally,
  and push. Do not guess from a red summary.
- Merge only after the relevant checks are green and the branch is current enough to
  avoid obvious conflicts.
- After merges to main, refresh the local baseline before selecting the next failures.

## Snapshot Discipline

Behavior PRs should not refresh conformance snapshots by default. Snapshot/readme PRs
are allowed when they document already-merged behavior changes.

Before a snapshot PR:

1. Sync to latest `origin/main`.
2. Run `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot`.
3. Confirm `scripts/conformance/conformance-snapshot.json`, detail data, and README
   agree.
4. Include exact pass/total numbers in the PR body.

## Disk and Worktree Discipline

- Generated build directories (`.target`, `target`, `node_modules`, `dist`) may be
  removed from inactive worktrees after validation.
- Before deleting a worktree, inspect `git status --short --branch`.
- Dirty worktrees must be salvaged or intentionally abandoned only after confirming the
  changed files do not advance the current conformance objective.
- Keep one clean integration/strategy worktree for documentation and orchestration.

## Active Queue

Open PRs to monitor without blocking local work:

| PR | Purpose | Status at last update |
| --- | --- | --- |
| #4400 | Simplify slow recursive conditional fixture | CI running |
| #4401 | Lock JSX excess prop assignability regression | CI running |
| #4402 | Show missing benchmark projects on website | CI running |
| #4395 | Unwitnessed recursive generic variance | Blocked by previous unit timeout until rerun/rebase |
| #4396 | Conformance regression locks | Blocked by previous unit timeout until rerun/rebase |

Active local workstreams:

| Workstream | Owner |
| --- | --- |
| Fingerprint-only diagnostics | agent worktree |
| Generic/contextual false positives | agent worktree |
| Parser/syntax diagnostics | agent worktree |
| Indexed access / `in` operator diagnostics | agent worktree |

## Completion Audit

The goal is complete only after all of these have concrete evidence:

- `origin/main` has `scripts/conformance/conformance-snapshot.json` with zero failures.
- README conformance block shows 100.0% for the same total.
- CI conformance shards and aggregate pass on the main commit containing the final
  snapshot.
- No open behavior PR needed for diagnostic conformance remains unmerged.
- A final local audit maps every remaining failure entry to zero in
  `scripts/conformance/conformance-detail.json`.
