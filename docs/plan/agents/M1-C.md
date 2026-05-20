# Agent Goal: M1-C

AgentName: M1-C
Computer: M1
Session: C
GitHub label: `agent:M1-C`

## Mission

Burn down diagnostic decisions based on rendered type strings, source snippets,
and file names. Replace them with structural facts from solver or
query-boundary helpers.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-C
scripts/agents/disk-preflight.sh M1-C
scripts/agents/list-owned-work.sh M1-C
```

## Current Assignment

- Primary issues: `#8228`, `#8775`, `#8286`.
- Related PRs to inspect: `#9272`, `#9162`, `#8983`, `#8972`.
- Track: roadmap Tracks 8 and 10.
- Next concrete step: pick one rendered-type/source-text decision, identify the
  structural fact it is trying to approximate, then expose or reuse a boundary
  query for that fact.

## Existing Work To Inspect First

- `#9272` and `#9162` both replace rendered-type decisions. Do not duplicate
  their scope.
- `#8983` covers JSX rendered type decisions.
- `#8972` touches builtin iterator display; inspect before changing display
  provenance around lib identities.

## Non-Overlap Rules

- Do not drive decisions from `format_type_diagnostic`, `contains`,
  `starts_with`, regexes, or conformance test names.
- Do not widen or mutate semantic types to make messages pretty. Presentation
  belongs in display policy.
- If the needed fact is semantic, expose it through solver/query boundaries.

## Verification

- Add focused tests for at least two name choices when binders are involved.
- Prefer targeted checker tests.
- Do not refresh broad snapshots as proof of this lane.
