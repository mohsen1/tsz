# Agent Goal: M4-Opus

AgentName: M4-Opus
Computer: M4
Session: Opus
GitHub label: `agent:M4-Opus`

## Mission

Own deep solver substrate work needed for all tests, green benchmarks, `2x`
performance, bug closure, and tech-debt burn-down. This lane handles
cross-cutting relation/evaluation/inference/identity/cache debt that ordinary
M4-A and M4-B lanes should not solve piecemeal.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M4-Opus release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-Opus
scripts/agents/disk-preflight.sh M4-Opus
scripts/agents/list-owned-work.sh M4-Opus
python3 scripts/arch/arch_guard.py
```

## Current Assignment

- Primary gates: all tests pass, solver-caused project rows turn green,
  semantic cache correctness is proven, and performance work has sound stable
  identity and bounded residency foundations.
- Bug or metric families: stable semantic identity, cache invalidation/reset
  boundaries, relation/evaluation cache keys, inference transaction state,
  recursive evaluation fuel, cross-file module/lib identity, and solver
  residency.
- Architecture cleanup metric: cache-key ambiguity, cross-arena identity
  misuse, oversized solver helpers, and unbounded file-session residency should
  trend down.
- First live command: inspect owned PRs, then identify a project-row,
  benchmark, or issue cluster where ordinary M4 lanes are blocked by shared
  substrate debt.
- Next concrete step: land one substrate PR with focused parity tests,
  cache/order-independence tests, and a measured debt or counter reduction.

## Existing Work To Inspect First

- Live `agent:M4-Opus` PRs and stale M4-labelled PRs that need takeover.
- M4-A/M4-B PRs or blockers asking for substrate help.
- `docs/architecture/INSTANTIATION_CACHE.md`,
  `docs/architecture/WELL_KNOWN_NAME_REFERENCES.md`,
  `docs/architecture/RELATION_REQUEST.md`, and
  `docs/plan/PERFORMANCE_PLAN.md`.
- Open issues labelled `solver`, `performance`, `tech-debt`, `bug`,
  `false-positive`, and `false-negative`.

## Non-Overlap Rules

- Do not take ordinary M4-A/M4-B work unless it needs shared substrate redesign
  or the owner asks for takeover.
- Do not tune performance by skipping diagnostics or weakening semantic parity.
- Do not introduce cache keys without naming mode fields, invalidation/reset
  boundaries, cycle/fuel behavior, and behavior when cache state is absent.
- Leave signed handoff comments when splitting work back to ordinary lanes.

## Verification

- Prefer focused solver tests with cache-enabled/cache-disabled or repeated-call
  variants.
- Use `cargo nextest run` filters; do not use `cargo test`.
- Run architecture guards when moving boundaries or guard caps.
- Use narrow project-row reductions only after focused unit invariants exist.
