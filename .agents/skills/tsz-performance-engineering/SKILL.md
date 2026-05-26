---
name: tsz-performance-engineering
description: Use when planning, implementing, reviewing, or interpreting TSZ performance work, including benchmark regressions, cache/residency changes, timing claims, perf counters, hotspot investigations, OOM/timeout/stack-overflow blockers, or optimization PR evidence.
---

# TSZ Performance Engineering

## Core Rule

Performance work must preserve `tsc` parity and TSZ architecture. Speed claims are useful only after the row or behavior is correct, except when the first blocker is runtime, OOM, timeout, stack overflow, or residency.

Before starting nontrivial perf work, read:

- `docs/plan/ROADMAP.md`
- `docs/plan/PERFORMANCE_PLAN.md`
- `references/perf-mistakes.md` when diagnosing a regression, designing a cache, writing PR evidence, or changing residency/session behavior

Also inspect open/recent PRs and issues for overlapping benchmark rows or perf tooling work.

## Classify First

Name the task class before coding:

- `correctness blocker`: the row is red/yellow due to diagnostics, crash, missing metadata, or fixture drift. Fix correctness first; do not claim speed progress.
- `runtime blocker`: OOM, timeout, stack overflow, excessive residency, or CI kill prevents the row from reaching a correctness result. Perf work is allowed with the runtime invariant stated.
- `green-row speed`: row is correct and complete; before/after timing can be meaningful.
- `cache/residency`: cache keys, invalidation, file sessions, project state, or memory growth. State the semantic identity and reset boundary.
- `measurement/tooling`: counters, reports, dashboard interpretation, or benchmark scripts. Preserve schema stability and low overhead.
- `micro-allocation`: clone, allocation, formatting, or buffer changes. Prove the hot path; do not overstate as project-row progress unless a row moves.

## Evidence Workflow

1. Record the affected project/benchmark row, bug family, and current correctness status.
2. Pick the narrowest command that answers the question; avoid broad local suites.
3. Separate timing mode from attribution mode. Do not compare attribution-mode TSZ to timing-mode `tsgo`.
4. For cache or residency changes, document every behavior-affecting key field, invalidation/reset boundary, semantic mode, cycle/fuel interaction, memory-growth expectation, and cache-disabled behavior.
5. For green-row speed claims, include before/after command, wall time, RSS/residency when relevant, cache/counter deltas, and noise sources.
6. For runtime blockers, include the failure class before/after and the invariant that prevents recurrence.
7. Add or update owning-crate tests for behavior changes. Let ready-for-review CI run heavy conformance, emit, fourslash, and WASM suites.

## Preferred Local Commands

Use filters and wrap long or memory-intensive runs with `scripts/safe-run.sh`.

```bash
python3 scripts/perf/cache-visibility-report.py --json
python3 scripts/perf/visited-clone-report.py --json
python3 scripts/perf/debug-print-report.py --json
python3 scripts/perf/migration_callsite_counts.py --json
python3 scripts/perf/query-perf-counters.py --json <artifact> --baseline <baseline>

scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --quick --json-file /tmp/hotspots.json
scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --filter '<row-or-fixture>' --json-file /tmp/bench.json
```

Use filtered project compile guard or focused `cargo nextest run -E 'test(...)'` when that is the shortest path to validate the change. Do not run full conformance, full emit, full fourslash, or broad project suites locally.

## Cache And Identity Checks

Before adding or widening a cache, answer:

- What semantic question does this cache answer?
- What stable identity is used? Avoid cross-file `NodeIndex` and do not compare `TypeId`s from different `TypeInterner`s.
- Does the key include every mode that changes behavior: relation mode, variance, freshness/excess-property state, contextual typing, inference source, `any` propagation policy, target/module/options, request scope, cycle/fuel, and file/session generation as applicable?
- Where is invalidation or reset performed?
- What happens when the cache is absent, cold, disabled, or order-randomized?
- Is size/residency bounded or observable?

Move semantic fixes through solver/query boundaries. Do not add checker-local type algorithms, display-string heuristics, source-text shortcuts, or name-only allowlists.

## PR Evidence Packet

Every perf PR body or status comment should include:

- `Project Corpus Impact`: row, bug family, correctness status, and evidence source
- `Invariant`: semantic identity, cache key, invalidation/reset, request scope, or residency rule
- `Verification`: exact local commands and any CI jobs relied on
- before/after timing only for complete green rows
- RSS/residency and failure-class evidence for runtime blockers
- cache/counter deltas for cache work
- noise sources and caveats
- `AgentName` and coordination notes for overlapping rows/PRs

Individual agents own their PR evidence and handoff notes. A manager or coordination agent should handle broad merge sequencing and cross-PR queue decisions.
