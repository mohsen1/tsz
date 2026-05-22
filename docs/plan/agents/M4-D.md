# Agent Goal: M4-D

AgentName: M4-D
Computer: M4
Session: D
GitHub label: `agent:M4-D`

## Mission

Stabilize symbol, lib, module, and cross-file identity. Replace name-only
allowlists with binder/solver identity facts.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-D
scripts/agents/disk-preflight.sh M4-D
scripts/agents/list-owned-work.sh M4-D
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8476`, `#8534`, `#8719`, `#8681`, `#6565`.
- Related PRs to inspect: `#9211`, `#9083`, `#8577`, `#8467`, `#8540`,
  `#8970`, `#8969`, `#8967`.
- Track: roadmap Track 7.
- Next concrete step: identify whether imported alias/type identity or builtin
  lib identity is the current smallest blocker, then choose one stable-identity
  query or boundary repair.

## Existing Work To Inspect First

- `#8577` uses `SymbolId` identity for Promise/PromiseLike detection.
- `#9211` resolves cross-file import types in conditional extends position.
- `#9083` resolves imported alias value annotations.
- `#8467` touches cross-arena `NodeIndex` collisions for multi-lib built-ins.

## Non-Overlap Rules

- Do not special-case builtin names with raw strings. Resolve through binder or
  stable builtin identity.
- Do not compare `TypeId`s across distinct interner universes.
- If module resolution config parsing is the issue, coordinate with
  Studio-A or the core tech-debt issue before changing semantics.

## Verification

- Include cross-file and alias/wrapper cases.
- Prefer targeted checker or binder tests.
- Do not run full conformance locally.
