# Agent Goal: Studio-E

AgentName: Studio-E
Computer: Studio
Session: E
GitHub label: `agent:Studio-E`

## Mission

Own JSDoc/JavaScript declaration emit parity and keep LSP/WASM work aligned
with compiler-service consumer boundaries. LSP and WASM consume compiler facts;
they do not own type algorithms.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh Studio-E
scripts/agents/disk-preflight.sh Studio-E
scripts/agents/list-owned-work.sh Studio-E
```

## Current Assignment

- Primary gate: JSDoc/JS declaration emit gaps plus low-bandwidth LSP/WASM
  consumer correctness.
- Bug families: JSDoc `@typedef`, `@satisfies`, `@implements`, JS declaration
  class/function/property output, hover/JSDoc display that consumes compiler
  summaries, and WASM/LSP filesystem or semantic-view consumer gaps.
- Architecture cleanup metric: JSDoc declaration facts should be normalized
  before DTS printing; LSP/WASM should converge on compiler-service semantic
  views rather than raw `TypeData` matching.
- First live command: inspect owned PRs, then search open issues for `jsdoc`,
  `declaration`, `hover`, `lsp`, `wasm`, and `compiler-service`.
- Next concrete step: take one JSDoc/JS declaration family or one consumer
  boundary smoke gap and keep it small enough not to steal checker/solver
  bandwidth.

## Existing Work To Inspect First

- Issues `#8720`, `#9333`, `#8275`, and open LSP/WASM consumer issues.
- `docs/plan/LSP_ROADMAP.md`.
- `docs/architecture/EMIT_ARCHITECTURE.md`.
- Recent JSDoc DTS and LSP hover/smoke PRs.

## Non-Overlap Rules

- Do not start broad LSP architecture rewrites while project rows or emit gates
  are red.
- If an LSP issue is a checker/solver semantic problem, route the fix to the
  owning compiler layer instead of patching the LSP response locally.
- JSDoc/DTS work should coordinate with Studio-D when it needs a shared
  declaration summary fact.
- WASM compatibility changes must avoid filesystem assumptions or guard them
  explicitly.

## Verification

- Prefer focused LSP tests, WASM smoke tests, or narrow DTS filters.
- Do not run full fourslash locally.
- Let ready-for-review CI run heavy gates.
