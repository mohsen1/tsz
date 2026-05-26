# TSZ Performance Mistakes And Guardrails

Load this reference when planning or reviewing nontrivial performance work.

## Current Strategy

The durable plan is project-row first. Broad optimization waits until required
rows are correct, except for runtime blockers where OOM, timeout, stack
overflow, or residency prevents a correctness result. A performance change must
state the invariant it preserves: semantic identity, cache key, invalidation,
request scope, file/session residency, and fallback behavior.

Do not treat a red/yellow row as a speed win. If diagnostics are wrong, metadata
is incomplete, artifacts are missing, or fixture provenance is unclear, the row
is not a timing target yet.

## Mistakes Seen Before

- **Faster but still wrong rows**: benchmark deltas were tempting before
  diagnostics and metadata were complete. Keep correctness status beside every
  timing number.
- **Attribution/timing mode mixing**: counter-enabled TSZ runs are for
  explanation, not direct comparison with timing-mode `tsgo`. Run timing and
  attribution as separate experiments.
- **Partial artifacts treated as truth**: memory-guard kills, missing
  compatibility fields, duplicate rows, or local fixture drift can make a
  dashboard row gray. Check row completeness before claiming movement.
- **Stack tuning instead of recursion fixes**: large rows have hit stack
  overflow from shape-specific unbounded recursion. Increasing stack size is
  not a root-cause fix; find the missing depth/fuel/cycle guard.
- **Cache keys missing modes**: relation mode, variance, freshness,
  contextual typing, inference source, `any` policy, target/module/options,
  cycle/fuel, or request scope can all change answers. Missing one creates
  stale correctness bugs.
- **Wrong identity**: `NodeIndex` is not cross-file semantic identity, and
  `TypeId`s from different `TypeInterner`s are not comparable. Stable
  cross-file reuse needs stable semantic facts.
- **Checker-local shortcuts**: performance pressure has led to type-shape
  recursion, display-string checks, source-text heuristics, and name-only lib
  allowlists. These belong in solver/query boundaries or binder/global facts.
- **Branch-local graph state explosions**: repeated `visited.clone()` traversal
  can turn graph walks into path explosion. Prefer memoized DP, SCC/worklists,
  or explicit bounds when the traversal is hot.
- **Counters causing their own regression**: perf counters must be gated,
  schema-stable, and cheap when disabled. Avoid production-cost atomics or
  formatting in hot paths.
- **Debug output in compiler internals**: `println!`, `eprintln!`, and `dbg!`
  distort runs and fail lint policy. Use structured tracing and the existing
  perf/debug-print reports.
- **Micro wins overclaimed**: preallocation, clone removal, and formatting
  fixes are valuable only when tied to a hot path or row movement. Keep claims
  proportional.
- **Stale coordination state**: old WIP text, issue notes, or PR labels may be
  stale. Use current PR head, row state, and fresh signed comments before
  taking ownership or publishing results.
- **Counter-accessor sprawl**: perf counter accessors are moving toward a
  declarative manifest. Do not duplicate accessor boilerplate; check the active
  issue/PR stack before editing counter surfaces.

## Tool Map

- `docs/plan/PERFORMANCE_PLAN.md`: current performance contract, measurement
  modes, evidence packet, and cache/residency rules.
- `docs/plan/ROADMAP.md`: when performance work is allowed relative to project
  rows and release gates.
- `scripts/bench/project-rows.mjs`: source of truth for required/canary rows
  and compatibility metadata.
- `scripts/bench/row-utils.mjs`: what counts as a complete green row.
- `scripts/bench/perf-hotspots.sh`: narrow hotspot suite for quick perf signal.
- `scripts/bench/bench-vs-tsgo.sh`: filtered TSZ vs `tsgo` project timing.
- `scripts/bench/tsgo-winner-report.mjs`: rows where `tsgo` is currently
  faster and known closure/owner context.
- `scripts/bench/project-winner-regression-report.mjs`: green rows that moved
  from TSZ winner to `tsgo` winner.
- `scripts/perf/query-perf-counters.py`: attribution artifact comparison.
- `scripts/perf/cache-visibility-report.py`: cache-like maps and size/stat
  observability gaps.
- `scripts/perf/visited-clone-report.py`: traversal paths likely to clone
  branch-local visited state.
- `scripts/perf/debug-print-report.py`: ad-hoc compiler debug output.
- `scripts/perf/migration_callsite_counts.py`: migration callsite counts for
  selected checker performance projects.

## Review Questions

Ask these before approving a performance PR:

1. Is the row correct and complete, or is this explicitly a runtime blocker?
2. Is the measurement mode named, and are timing and attribution separated?
3. Does the PR preserve `tsc` parity and architecture boundaries?
4. Are cache key fields, invalidation/reset, request scope, and fallback behavior documented?
5. Are RSS/residency or failure-class deltas included when memory/runtime is the point?
6. Are claims limited to what the evidence proves?
7. Did the agent check overlapping open/recent PRs and issues?
