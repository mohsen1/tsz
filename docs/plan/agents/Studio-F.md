# Agent Goal: Studio-F

AgentName: Studio-F
Computer: Studio
Session: F
GitHub label: `agent:Studio-F`

## Mission

Own the multi-session runway: disk, worktree reuse, TypeScript submodule reuse,
Cargo-cache-preserving cleanup, stuck CI hygiene, and launch-plan updates.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-F
scripts/agents/disk-preflight.sh Studio-F
scripts/agents/list-owned-work.sh Studio-F
```

## Current Assignment

- Primary issues: `#8428`, `#8282`, plus this coordination PR while active.
- Related PRs to inspect: `#9280`, `#9270`, `#9262`, `#9258`, `#9254`.
- Track: roadmap Track 10.
- Next concrete step: keep this launch system mergeable, then make sure every
  computer can run `disk-preflight`, reuse a populated `TypeScript/` submodule,
  and clean safely without losing useful Cargo caches.

## Existing Work To Inspect First

- `#9270` added memory-guard pre-push tests.
- `#9262`, `#9258`, and `#9254` updated safe-run and conformance tooling docs.
- The local disk guard may report low disk; prefer reuse and cache-preserving
  cleanup over new worktrees.

## Non-Overlap Rules

- Do not run broad disk archaeology commands as routine status.
- Do not recommend `cargo clean` for ordinary cleanup.
- Do not delete worktrees unless their owner/branch/PR status is understood.
- Do not change roadmap direction for routine launch bookkeeping.

## Verification

- Test helper scripts directly with `--help` or harmless listing modes.
- Use `scripts/setup/clean.sh --dry-run` before changing cleanup guidance.
- No compiler suite is needed for docs/script launch changes.
