# Agent Goal: M1-B

AgentName: M1-B
Computer: M1
Session: B
GitHub label: `agent:M1-B`

## Mission

Move checker relation diagnostics onto shared relation/query-boundary
entrypoints. Preserve `TS2322`/`TS2345`/`TS2416` parity while reducing raw
boolean assignability plus local post-checks.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-B
scripts/agents/disk-preflight.sh M1-B
scripts/agents/list-owned-work.sh M1-B
```

## Current Assignment

- Initial priority: land, close, or clearly hand off existing PRs in this lane
  before claiming issue backlog.
- Issue context: `#8227`, `#8225`, `#8223`.
- Related active stack: `#9222`, `#9236`, `#9238`, `#9242`, `#9245`,
  `#9247`, `#9306`, `#9315`.
- Track: roadmap Tracks 4 and 10.
- Next concrete step: inspect the stack, identify the next smallest checker
  relation guard that can route through an existing boundary helper, then either
  continue the stack or comment why a duplicate draft should close.

## Existing Work To Inspect First

- Ready or recent relation work: `#9302`, `#9300`, `#9226`.
- Long-running checker boundary PRs: `#9148`, `#9009`, `#9008`.
- Avoid changing solver relation policy internals directly; that is M4-B.

## Non-Overlap Rules

- New checker code must not call `CompatChecker` directly for TS2322-family
  paths when a boundary helper can exist.
- If the fix needs relation cache-key or policy changes, hand off to M4-B or
  stack on its branch instead of mixing concerns.
- Every behavior-changing PR states the structural rule and adjacent cases.

## Verification

- Prefer targeted checker tests or narrow `cargo nextest run -p tsz_checker`.
- Do not run full conformance locally.
- If a branch only changes routing, state why behavior is unchanged.
