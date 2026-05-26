# Agent Goal: M1-B

AgentName: M1-B
Computer: M1
Session: B
GitHub label: `agent:M1-B`

## Mission

Move checker relation diagnostics onto shared relation/query-boundary
entrypoints. Preserve `TS2322`/`TS2345`/`TS2416` parity while reducing raw
boolean assignability plus local semantic post-checks.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-B
scripts/agents/disk-preflight.sh M1-B
scripts/agents/list-owned-work.sh M1-B
```

## Current Assignment

- Primary gate: all bugs fixed on checker relation diagnostic paths.
- Bug families: assignment, argument, override, call/property relation, excess
  property, weak type, and missing-property diagnostics that already have a
  solver answer or need a checker-facing boundary.
- Architecture cleanup metric: direct checker relation call sites that need
  relation result plus structured reason should trend down; `query_boundaries`
  should become request-shaped APIs rather than quarantine barrels.
- First live command: inspect owned PRs, then query issues around `TS2322`,
  `TS2345`, `TS2416`, `RelationRequest`, and checker `tech-debt`.
- Next concrete step: choose one checker call path and route it through an
  existing or narrow new boundary helper without changing solver policy.

## Existing Work To Inspect First

- Open `agent:M1-B` PRs and recent merged checker relation PRs.
- Issues `#8227`, `#8225`, and `#8223` for durable boundary debt context.
- `docs/architecture/RELATION_REQUEST.md` and
  `docs/architecture/QUERY_BOUNDARY_INVENTORY.md`.
- M4-B work on relation policy/cache keys before changing solver internals.

## Non-Overlap Rules

- New checker code must not call `CompatChecker` directly for TS2322-family
  paths when a boundary helper can exist.
- If the fix needs variance, any propagation, relation policy, or cache-key
  semantics, hand off to M4-B or stack explicitly.
- Every behavior-changing PR states the structural rule and adjacent cases.
- Do not hide a diagnostic mismatch behind rendered type strings or file names.

## Verification

- Prefer targeted checker tests or narrow `cargo nextest run -p tsz_checker`.
- Use architecture guards when boundary code changes:
  `scripts/arch/check-checker-boundaries.sh` and
  `python3 scripts/arch/arch_guard.py`.
- Do not run full conformance locally.
