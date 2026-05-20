# TSZ Performance Plan

Status: roadmap appendix. `docs/plan/ROADMAP.md` owns active sequencing,
release gates, and whether performance work is top-line. This file defines the
durable measurement and review contract for performance, residency, and cache
work.

Do not use this file as a run log, dated benchmark ledger, or branch-local
status page. Put run output in PR bodies, PR comments, CI artifacts, or local
scratchpads. Update this file only when the performance strategy, guardrails,
measurement workflow, or cache/residency review rules change durably.

## Performance North Star

Performance work must preserve `tsc` parity. Large-project speed comes from
stable semantic identity, explicit request scopes, bounded residency, and
auditable caches. It must not come from checker-local shortcuts that bypass
solver semantics, source-text heuristics, stale cache answers, or skipped
diagnostics.

Correctness comes first:

1. Red or yellow project rows do not get speed claims unless the first blocker
   is explicitly runtime, OOM, timeout, or residency.
2. Green rows can be timed against `tsgo` in timing mode.
3. A performance PR that changes semantics is a semantic PR first and must meet
   the owning checker/solver/emit review rules from `ROADMAP.md`.
4. A performance PR that changes only allocation/capacity/cache shape must still
   state the semantic invariant that keeps behavior unchanged.

## Measurement Modes

| Mode | Purpose | Counters/tracing | Comparable to `tsgo` timing? |
| --- | --- | --- | --- |
| `timing` | Wall time and RSS claims | Off | Yes |
| `attribution` | Explain where time goes | On | No |

Never compare attribution-mode `tsz` directly against timing-mode `tsgo`.
Counter paths that can call timing APIs must be compiled out of timing builds
or otherwise proven absent from timing profiles.

## Required Evidence In Performance PRs

Record the following in the PR body when the PR is performance-motivated:

1. benchmark row, project row, or fixture name,
2. before/after command,
3. measurement mode,
4. wall time when timing is claimed,
5. peak RSS or physical footprint when residency changes,
6. diagnostic/project-row status before and after,
7. cache/counter deltas when the change is counter-driven,
8. known noise sources,
9. semantic identity, cache-key, or invalidation invariant protected by the
   change.

For red/yellow rows, record the first correctness blocker and leave timing
claims blank unless runtime, OOM, timeout, or residency is the blocker.

## Preferred Commands

Use the narrowest command that answers the question:

```bash
scripts/bench/perf-hotspots.sh --quick
scripts/bench/bench-vs-tsgo.sh --filter '<fixture>'
TSZ_PROJECT_COMPILE_FILTER='<row-regex>' scripts/ci/project-compile-guard.sh
cargo nextest run -p <crate> -- <test-filter>
```

Wrap heavy or multi-worker commands:

```bash
scripts/safe-run.sh cargo nextest run
scripts/safe-run.sh scripts/bench/bench-vs-tsgo.sh --filter '<fixture>'
```

Do not run full conformance, full emit, or full fourslash locally.

## Cache And Residency Contracts

Every cache-changing PR must state:

1. cache key fields,
2. invalidation/reset boundary,
3. semantic modes that can change the answer,
4. cycle/fuel behavior when relevant,
5. memory-growth expectation,
6. how behavior is preserved when cache state is absent or disabled.

Durable constraints:

1. `NodeIndex` is a syntax traversal coordinate, not cross-file semantic
   identity.
2. Cross-file semantic reuse should be keyed by stable semantic identity.
3. Do not intern substitution environments on `TypeInterner`.
4. Preserve cheap leaf fast paths before constructing expensive cache keys.
5. Do not compare `TypeId`s across distinct `TypeInterner`s in tests.
6. Cache keys must include every semantic mode that can change the answer.
7. Performance counters are evidence, not policy.

## File-Session Direction

The target shape is bounded file-session reuse:

1. long-lived project facts and caches are shared,
2. file-local state resets at file boundaries,
3. speculative/request state is transaction-scoped,
4. full AST/binder residency becomes a fallback rather than the default answer
   path.

Session reuse must prove diagnostic stability. Constructor-count reductions are
useful evidence, but green project-corpus rows are the stronger signal.

## Disk And Build Cache Hygiene

Multi-session performance work is disk-sensitive. Do not use `cargo clean` as
routine hygiene because it destroys useful compile state.

Preferred cleanup ladder:

1. `scripts/setup/disk-worktree-guard.sh`
2. `scripts/setup/disk-worktree-guard.sh --auto-prune` when disk is low
3. `scripts/setup/clean.sh --quiet`
4. delete only abandoned worktrees whose branch/PR owner is understood
5. `scripts/setup/clean.sh --full` only as a deliberate last resort

`scripts/setup/clean.sh` without `--full` preserves `.target/`,
`.target-bench/`, and `target/`, and prunes stale Cargo incremental
directories older than seven days.

Reuse worktrees with populated `TypeScript/` and Cargo caches whenever possible.
In sibling worktrees, prefer `scripts/setup/link-ts-submodule.sh` so the
TypeScript corpus is shared from the primary checkout instead of cloned again.

## Merge Discipline

Performance work is only useful after it lands on `main`.

Before opening or reviving a performance branch:

1. fetch `origin/main`,
2. inspect open and recently merged PRs for overlap,
3. reuse an existing worktree when possible,
4. rebase or merge the branch onto current `main`,
5. close or consolidate duplicate chore branches,
6. record the exact verification command that protects the changed invariant.

After opening a PR:

1. keep the branch synchronized with `main`,
2. treat failing required checks as the next task before starting new work in
   the same lane,
3. avoid stacking broad performance PRs behind unmerged ready PRs,
4. delete remote branches after merge or after closing duplicate work.
