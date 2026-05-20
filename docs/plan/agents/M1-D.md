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

- Primary issues: `#8780`, `#8424`.
- Related PRs to inspect: `#8919`, `#8903`, `#8982`, `#8661`, `#8470`.
- Track: roadmap Track 6.
- Next concrete step: determine whether the destructured-discriminant work has
  a live owner. If not, take the smallest repro path and preserve the rule with
  a checker or solver test.

## Existing Work To Inspect First

- `#8919` and `#8903` both touch destructured or aliased discriminant
  behavior.
- `#8982` handles class property initializer narrowing reset.
- `#8661` handles `Symbol.hasInstance`-aware `instanceof` narrowing.

## Non-Overlap Rules

- Do not patch one variable name or one test file. State the flow fact
  structurally.
- If a narrowing operation needs semantic type computation, route it to solver
  predicates or query-boundary helpers.
- Keep flow-state resets local and explicit.

## Verification

- Prefer focused checker narrowing tests.
- Include negative cases that prove unsupported flow shapes do not silently
  narrow.
- Do not run full conformance locally.
