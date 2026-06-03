# Agent Goal: Studio-Opus

AgentName: Studio-Opus
Computer: Studio
Session: Opus
GitHub label: `agent:Studio-Opus`

## Mission

Own deep Studio-side blockers across project corpus, benchmark infrastructure,
emit/DTS architecture, LSP/WASM/compiler-service boundaries, and tech-debt
burn-down. This lane handles cross-cutting Studio work that spans Studio-A,
Studio-B, and Studio-C.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next Studio-Opus release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-Opus
scripts/agents/disk-preflight.sh Studio-Opus
scripts/agents/list-owned-work.sh Studio-Opus
node scripts/bench/project-row-summary.mjs --markdown
python3 scripts/emit/query-emit.py --families
python3 scripts/emit/audit-output-surgery.py
python3 scripts/arch/arch_guard.py
```

## Current Assignment

- Primary gates: all benchmark/project rows are truthful and green when `tsc`
  accepts them, all eligible green timed rows can prove the `2x` target, JS/DTS
  emit reaches `100%`, and Studio-side tech debt decreases with evidence.
- Bug or metric families: project-row metadata drift, benchmark artifact
  schema/summary gaps, output-surgery pressure, declaration summary
  architecture, compiler-service/LSP/WASM consumer boundaries, large fixture
  reliability, and release metric publication.
- Architecture cleanup metric: project-row metadata drift, stale benchmark
  artifact fields, output-surgery allowlist pressure, emit reach-through, and
  consumer-boundary leaks should trend down.
- First live command: inspect owned PRs, then compare project-row summary,
  emit families, output-surgery audit, and architecture guard output for a
  cross-Studio blocker.
- Next concrete step: land one infrastructure or boundary PR that lets ordinary
  Studio lanes make stronger correctness, emit, or performance claims.

## Existing Work To Inspect First

- Live `agent:Studio-Opus` PRs and stale Studio-labelled PRs that need
  takeover.
- Studio-A/B/C PRs or blockers asking for cross-Studio help.
- `docs/plan/PERFORMANCE_PLAN.md`,
  `docs/architecture/EMIT_ARCHITECTURE.md`, and `docs/plan/LSP_ROADMAP.md`.
- Benchmark, emit, compile-guard, LSP, WASM, and website metric scripts.

## Non-Overlap Rules

- Do not take ordinary Studio-A/B/C work unless it crosses project metrics,
  emit architecture, benchmarks, or consumer boundaries.
- Do not publish new metric claims without artifact fields or CI URLs.
- Do not add output-surgery debt without an owner, removal condition, and
  counter update.
- Leave signed handoff comments when splitting work back to ordinary lanes.

## Verification

- Prefer script tests under `scripts/bench`, `scripts/emit`, `scripts/ci`, and
  `scripts/lsp`.
- Use `node scripts/bench/validate-project-metadata.mjs` for row metadata.
- Run `python3 scripts/emit/audit-output-surgery.py` when touching emit
  shortcuts or output-surgery allowlists.
- Wrap heavy benchmark or project checks with `scripts/safe-run.sh`.
