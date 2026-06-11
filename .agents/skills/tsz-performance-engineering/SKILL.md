---
name: tsz-performance-engineering
description: Use when planning, implementing, reviewing, or interpreting TSZ performance work, including benchmark regressions, cache/residency changes, timing claims, perf counters, hotspot investigations, OOM/timeout/stack-overflow blockers, or optimization PR evidence.
---

# TSZ Performance Engineering

Use for performance, cache/residency, OOM/timeout/stack-overflow, counters,
benchmark regressions, and timing claims.

## Rules

- Read `AGENTS.md`, `docs/plan/ROADMAP.md`, and relevant PR/issues.
- Correctness first. Timing claims matter only for green rows or explicit
  runtime/residency blockers.
- Optimize the repeated operation, not a fixture spelling.
- State invariant: cache key, invalidation/reset, request scope, fuel/cycle
  behavior, residency bound, or complexity change.
- No checker-local semantic algorithms, display-string heuristics, source-text
  shortcuts, name allowlists, or cross-interner `TypeId` comparisons.
- Read `references/perf-mistakes.md` before adding/widening caches, changing
  residency, or claiming speedups.

## Evidence

Use narrow, reproducible commands; wrap heavy runs.

```bash
python3 scripts/perf/cache-visibility-report.py --json
python3 scripts/perf/visited-clone-report.py --json
python3 scripts/perf/debug-print-report.py --json
python3 scripts/perf/migration_callsite_counts.py --json
python3 scripts/perf/query-perf-counters.py --json <artifact> --baseline <baseline>
scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --quick --json-file /tmp/hotspots.json
scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --filter '<row>' --json-file /tmp/bench.json
scripts/bench/measure-tsz.sh --timeout 420 --json-file /tmp/m.json -- --noEmit -p <tsconfig>
```

Use focused compile guard or `cargo nextest run -E 'test(...)'` when shortest.
Do not run full conformance, emit, fourslash, or broad project suites locally.

For ad-hoc timing or perf bisects on shared boxes, use
`scripts/bench/measure-tsz.sh`: it snapshots the binary to an immutable
hash-verified copy (never time the live `dist-fast/` path — sibling sessions
overwrite it) and records process CPU time next to wall time, so wall-only
timeouts under CPU contention are reported as unmeasured instead of slow.
See `references/perf-mistakes.md` and issue #13174.

## Cache Checklist

- What semantic question is cached?
- What stable identity is the key? Avoid cross-file `NodeIndex` and
  cross-interner `TypeId`.
- Does the key include all behavior modes: relation, variance, freshness,
  contextual typing, inference source, `any`, target/module/options, request
  scope, cycle/fuel, file/session generation?
- Where is reset/invalidation?
- What happens cold/disabled/order-randomized?
- Is size/residency bounded or observable?

PR/comment packet: goal (usually `fast`) and affected rows, invariant, exact
commands/CI, green row timing only, RSS/failure-class evidence for runtime
blockers, counter deltas, noise/caveats.
