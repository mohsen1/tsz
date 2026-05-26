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

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Track: roadmap Track 6.
- Current owned PR queue:
  - `#10075`: ready refactor PR; wait for exact-head required CI, then land or
    fix any failing check.
  - `#9933`: ready conditional-break narrowing PR; wait for exact-head heavy
    CI/queue, then land or fix any failing check.
  - `#9937`: draft enum equality narrowing PR; promote only after exact-head
    draft-light CI is clean, then let ready-review CI be the landing authority.
  - `#9889`: draft do-while narrowing PR stacked on `#9933`; keep parked until
    `#9933` lands or the stack is otherwise resolved, then rebase onto `main`
    and mark ready after focused verification.
- Historical issue context: `#8780` and `#8424` are closed; related PRs
  `#8919`, `#8903`, `#8982`, and `#8470` are merged, while `#8661` was closed.
- Next concrete step after the owned PR queue is clear: search for an open
  Track 6 narrowing or flow-predicate issue with no live owner. If one exists,
  take the smallest structural repro path and preserve the rule with checker or
  solver tests.

## Existing Work To Inspect First

- Inspect the current `agent:M1-D` PRs first; they are the active ownership
  surface for this lane.
- Treat `#8919`, `#8903`, `#8982`, `#8661`, and `#8470` as historical context
  only unless a new open issue explicitly depends on them.

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
