# Agent Goal: M4-D

AgentName: M4-D
Computer: M4
Session: D
GitHub label: `agent:M4-D`

## Mission

Stabilize symbol, lib, module, `DefId`, and cross-file identity. Replace
name-only allowlists with binder/solver identity facts.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M4-D
scripts/agents/disk-preflight.sh M4-D
scripts/agents/list-owned-work.sh M4-D
```

## Current Assignment

- Primary gate: all bugs fixed for stable semantic identity across files,
  libs, globals, imports, declarations, and module graphs.
- Bug families: `import()` types, namespace/enum merging, module augmentation,
  DOM/lib globals, well-known symbols, alias owners, `DefId` mapping, class
  static/instance identity, declaration-module exports, and display provenance
  over stable identity.
- Architecture cleanup metric: raw name allowlists, actual-lib alias
  admissions, cross-arena `TypeId` comparisons, and ad hoc symbol fallbacks
  should trend down.
- First live command: inspect owned PRs, then search open issues for
  `module`, `import`, `lib`, `symbol`, `unique symbol`, `DefId`, `alias`, and
  `well-known`.
- Next concrete step: choose one identity repair that replaces a string/name
  shortcut with binder/global or solver identity.

## Existing Work To Inspect First

- `docs/architecture/WELL_KNOWN_NAME_REFERENCES.md`.
- `docs/architecture/DEFID_RAW_SYMBOL_FALLBACK_PRODUCERS.md`.
- Cross-file import, builtin lib identity, and module-resolution recent PRs.
- Studio-A when fixture/module config is the real project-row blocker.

## Non-Overlap Rules

- Do not special-case builtin names with raw strings. Resolve through binder,
  stable builtin identity, or protocol query helpers.
- Do not compare `TypeId`s across distinct interner universes.
- If module resolution config parsing is the issue, coordinate with Studio-A or
  the core module-resolution tech-debt lane before changing semantics.

## Verification

- Include cross-file and alias/wrapper cases.
- Prefer targeted checker or binder tests.
- Run architecture guards when replacing identity fallbacks.
- Do not run full conformance locally.
