# TSZ Performance Notes

Status: retained path for code/script references. The active execution roadmap
is `docs/plan/ROADMAP.md`, especially Track 10: Guardrails, Tooling,
Residency, And Performance Substrate.

The previous performance plan accumulated dated run logs, claim-file links, and
branch-local status. That tracking now belongs in GitHub draft PRs, PR comments,
CI artifacts, and benchmark dashboard output. This file keeps the durable
performance contracts that code comments and scripts can safely reference.

## Performance North Star

Performance work must preserve `tsc` parity. Large-project speed comes from
stable semantic identity, explicit requests, bounded residency, and auditable
caches, not from checker-local shortcuts that bypass solver semantics.

## Required Measurements

For performance-motivated PRs, record in the PR body:

1. benchmark or fixture name,
2. before/after command,
3. wall time when relevant,
4. peak RSS or physical footprint when the change affects residency,
5. diagnostic count or project-corpus status before and after,
6. cache/counter deltas when the change is counter-driven,
7. known noise sources or why a run is attribution-only.

Use `scripts/safe-run.sh` for memory-intensive or long-running commands.

## Merge Discipline

Performance work is only useful after it lands on `main`; do not optimize for a
large stack of unmerged PRs. Each performance PR must stay small enough to merge
quickly and must name the owning agent in the PR body.

Before opening or reviving a performance branch:

1. fetch `origin/main`,
2. inspect open and recently merged PRs for overlap,
3. rebase or merge the worktree onto current `main`,
4. close or consolidate duplicate chore branches instead of leaving parallel
   cleanup PRs open,
5. record the exact verification command that protects the changed invariant.

After opening a PR:

1. enable auto-merge as soon as required checks are expected to pass,
2. keep the branch synchronized with `main` whenever `main` moves or CI reports
   a merge conflict,
3. treat failing required checks as the next task before starting new
   performance work,
4. avoid opening additional performance PRs while earlier ready PRs are only
   waiting for merge-queue or CI capacity,
5. delete remote branches after merge or after closing duplicate work.

Set an external timer or CI/status reminder for active performance PRs and
refresh it when `main` advances. The reminder is part of the work: stale PRs are
considered unfinished even when the code change is otherwise complete.

Distinguish timing evidence from attribution evidence:

| Mode | Purpose | Counter state | Comparable to `tsgo` timing? |
| --- | --- | --- | --- |
| `timing` | Wall time and RSS claims | Off | Yes |
| `attribution` | Explain where time goes | On | No |

Never compare attribution-mode `tsz` directly against timing-mode `tsgo`.
Counter paths that can call timing APIs must be compiled out of timing builds
or otherwise proven absent from timing profiles.

## Benchmark Families

Use the narrowest command that answers the question:

- `scripts/bench/perf-hotspots.sh --quick` for hot family checks.
- `scripts/bench/bench-vs-tsgo.sh --filter '<fixture>'` for project or
  library fixture checks.
- `scripts/ci/project-compile-guard.sh` for CI-style project compile guards.
- Targeted unit or integration tests when validating a semantic invariant.

For project benchmark harness changes, measure failed-row overhead as part of
the benchmark contract. A project row that fails pre-validation should record
exit class and first diagnostic deltas from the first compiler invocation; it
should not rerun the failing compiler just to populate compatibility metadata.
Generated flat project `tsconfig` files are benchmark-owned artifacts and must
be rewritten whenever their fixture is prepared. External fixture caches may
outlive script revisions; stale generated configs can invalidate project rows
by keeping old include/exclude rules.

Do not run full conformance, full emit, or full fourslash locally.

## Project Compile Guard Contract

`project-compile-guard.sh` is the cheap compile-only gate for rows that are
already fixture-reduced enough for CI. It must work in the same workspaces used
by developers and CI:

1. The default `TSZ_BIN` follows `CARGO_TARGET_DIR` when that environment
   variable is set, otherwise it falls back to `.target/dist-fast/tsz`.
2. `TSZ_PROJECT_COMPILE_FILTER='<row-regex>'` runs only matching rows, so agents
   can verify one project without cloning or checking every earlier fixture.
3. Generated app prerequisites (`node` and `npm`) are required only when a
   generated app row is selected.

Use the guard to answer "does this row compile?" Do not treat it as the full
benchmark dashboard: rows like `zod-project` can still fail in
`bench-vs-tsgo.sh` even when the compile guard's smaller row set is green. A
current `zod-project` failure is the contextual generic-constructor path at
`src/types.ts:280`, where tsz widens the nested `Effect<any>` discriminant and
emits `TS2322` while `tsc` accepts the project. That belongs to checker/solver
contextual typing, not benchmark harness code.

## Durable Design Constraints

1. `NodeIndex` is a syntax traversal coordinate, not cross-file semantic
   identity.
2. Binder/project skeletons should own stable declaration locations and
   topology facts.
3. Checker should rehydrate syntax only when source traversal is necessary.
4. Cross-file semantic reuse should be keyed by stable semantic identity.
5. `QueryCache` owns query/cache state that must be clearable and measurable.
6. Do not intern substitution environments on `TypeInterner`.
7. Preserve cheap leaf fast paths before constructing expensive cache keys.
8. Do not compare `TypeId`s across distinct `TypeInterner`s in tests.
9. Cache keys must include every semantic mode that can change the answer.
10. Performance counters are evidence, not policy; the policy belongs in
    architecture and roadmap docs.

## File-Session And Residency Direction

The target shape is bounded file-session reuse:

1. long-lived project facts and caches are shared,
2. file-local state resets at file boundaries,
3. speculative/request state is transaction-scoped,
4. full AST/binder residency becomes a fallback rather than the default answer
   path.

Session reuse must prove diagnostic stability. Constructor-count reductions are
useful evidence, but green project-corpus rows are the stronger signal.

## Perf Counter Hygiene

Counters should be:

1. cheap when disabled,
2. named after semantic or architectural events,
3. stable enough for PR-to-PR comparison,
4. dumped through machine-readable JSON when used for evidence,
5. removed or demoted when the tracked migration is complete.

Do not keep repo-local dated raw run dumps as planning artifacts. Attach bulky
artifacts to PRs or CI runs instead.
