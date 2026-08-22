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
- Do not change semantic sequencing merely to make a benchmark faster.
- No display-string heuristics, source-text shortcuts, name allowlists,
  cross-session `TypeId` comparisons, or hidden eager materialization.
- Start semantic operations uncached. A new cache needs a typed key, explicit
  dependencies/lifetime, a residency bound, and uncached agreement tests.
- One checker session owns recursion identity and budgets. Do not add a local
  force entry point, recursion stack, or fuel counter for a timeout/overflow;
  route the demand through the existing evaluation owner and stop on incomplete
  operands.
- Relation/projection traversal depth is not evaluator fuel. Keep budget axes
  typed and session-owned; never seed nested forcing from caller depth or reset
  it at zero.
- Inventory direct `.force_type`/`.force_deferred` calls, raw root resets, and
  required-type whole-tree passes before changing termination behavior.
- Run `python3 scripts/arch/rewrite_architecture_metrics.py --check`; performance
  work must not grow the forcing, recursion-owner, or side-table ratchets.

## Evidence

Use narrow, reproducible commands; wrap heavy runs.

```bash
scripts/safe-run.sh ./scripts/bench/perf-hotspots.sh --quick --json-file /tmp/hotspots.json
scripts/safe-run.sh ./scripts/bench/bench-vs-tsgo.sh --filter '<row>' --json-file /tmp/bench.json
scripts/bench/measure-tsz.sh --timeout 420 --json-file /tmp/m.json -- --noEmit -p <tsconfig>
```

`scripts/perf/query-perf-counters.py` reads the retired compiler's attribution
schema. Keep it as historical harness evidence, but do not use it to interpret
rewrite counters until its schema is explicitly ported to the replacement.

Use focused compile guard or `cargo nextest run -E 'test(...)'` when shortest.
Do not run full conformance, emit, fourslash, or broad project suites locally.

For ad-hoc timing or perf bisects on shared boxes, use
`scripts/bench/measure-tsz.sh`: it snapshots the binary to an immutable
hash-verified copy (never time the live `dist-fast/` path — sibling sessions
overwrite it) and records process CPU time next to wall time, so wall-only
timeouts under CPU contention are reported as unmeasured instead of slow.
The wrapper also records CPU share so host contention is not misclassified as
a compiler regression.

## Cache Checklist

- What semantic question is cached?
- What stable identity is the key? Avoid cross-file syntax coordinates and
  cross-session `TypeId`.
- Does the key include all behavior modes: relation, variance, freshness,
  contextual typing, inference source, `any`, target/module/options, request
  scope, cycle/fuel, file/session generation?
- Which declarations/options does it depend on, and where is reset/invalidation?
- What happens cold/disabled/order-randomized?
- Is size/residency bounded or observable?

PR/comment packet: goal (usually `fast`) and affected rows, invariant, exact
commands/CI, green row timing only, RSS/failure-class evidence for runtime
blockers, counter deltas, noise/caveats.
