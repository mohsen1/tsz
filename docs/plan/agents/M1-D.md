# Agent Goal: M1-D

AgentName: M1-D
Computer: M1
Session: D
GitHub label: `agent:M1-D`

## Mission

Close narrowing and flow predicate parity gaps without creating a second type
evaluator in checker flow code. Checker supplies flow facts and locations;
solver-owned predicates compute narrowed types.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-D
scripts/agents/disk-preflight.sh M1-D
scripts/agents/list-owned-work.sh M1-D
```

## Current Assignment

- Primary gate: all bugs fixed for flow/narrowing behavior that affects
  project rows, conformance strictness, or real project reductions.
- Bug families: discriminated unions, destructured discriminants,
  user-defined predicates, `in` narrowing, optional/truthiness narrowing,
  array/object guards, exhaustive switch behavior, alias-aware flow facts, and
  Kysely/Zod guard reductions.
- Architecture cleanup metric: direct checker narrowing semantics should move
  behind solver-owned predicates or narrow query-boundary helpers.
- First live command: inspect owned PRs, then search open issues for
  `narrow`, `flow`, `predicate`, `discriminant`, `guard`, `Kysely`, and `Zod`.
- Next concrete step: reduce one bug family to a structural rule and add a
  focused checker/solver test before changing implementation.

## Existing Work To Inspect First

- Open and recently merged `agent:M1-D` PRs.
- Prior destructured/aliased discriminant, class initializer narrowing reset,
  and `Symbol.hasInstance` work.
- `crates/tsz-checker/src/query_boundaries/flow*.rs` and solver narrowing APIs.

## Non-Overlap Rules

- Do not patch one variable name or one test file.
- If a narrowing operation needs semantic type computation, route it to solver
  predicates or query-boundary helpers.
- Keep flow-state resets local and explicit.
- Coordinate with M4-A/M4-C when the apparent flow bug is actually deferred
  evaluation or generic inference.

## Verification

- Prefer focused checker narrowing tests.
- Include negative cases that prove unsupported flow shapes do not silently
  narrow.
- Do not run full conformance locally.
