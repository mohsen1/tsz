# Agent Goal: M4-A

AgentName: M4-A
Computer: M4
Session: A
GitHub label: `agent:M4-A`

## Mission

Own recursive conditional, mapped, and alias identity evaluation bugs that block
project rows and conformance parity. Keep evaluation solver-owned and avoid
checker symptom patches.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-A
scripts/agents/disk-preflight.sh M4-A
scripts/agents/list-owned-work.sh M4-A
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#9305`, `#8778`, `#8726`, `#8702`, `#8687`, `#8423`,
  `#7648`.
- Related PRs to inspect: `#9304`, `#9214`, `#9210`, `#9006`, `#8985`,
  `#8939`, `#8904`.
- Track: roadmap Track 2.
- Next concrete step: reconcile `#9305` with `#9304` and recent recursion
  identity work, then choose one structural identity invariant that can land as
  a small solver PR.

## Existing Work To Inspect First

- Recent merge `#9296` changed recursion identity for deeply nested conditional
  mapped types.
- `#9304` is ready and should be drained before starting overlapping work.
- `#9214`, `#8985`, and `#9006` overlap mapped alias identity.

## Non-Overlap Rules

- Do not add test-name, alias-name, or display-string special cases.
- Do not erase deferred conditionals to `any` or `error` to silence one
  diagnostic.
- If the issue is a cache-key/policy bug, coordinate with M4-B.

## Verification

- Add solver or checker tests with renamed type parameters and alias/wrapper
  variants.
- Use narrow `cargo nextest run` filters.
- Do not run full conformance locally.
