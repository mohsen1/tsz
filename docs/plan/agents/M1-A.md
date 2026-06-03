# Agent Goal: M1-A

AgentName: M1-A
Computer: M1
Session: A
GitHub label: `agent:M1-A`

## Mission

Own checker diagnostic conformance, accepted-regression burn-down, and
diagnostic hardcoding debt. Keep diagnostic parity at `100%` while replacing
rendered/source-text/test-name shortcuts with structural checker or query
facts.

This goal is not complete when a branch exists. Keep going until the scoped
change lands in `main`, then pick the next M1-A release-gate item.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-A
scripts/agents/disk-preflight.sh M1-A
scripts/agents/list-owned-work.sh M1-A
python3 scripts/conformance/query-conformance.py --dashboard
```

## Current Assignment

- Primary gates: all tests pass, diagnostic conformance stays at `100%`,
  accepted-regression entries are removed or freshly justified, and diagnostic
  bug issues are fixed or structurally owned.
- Bug or metric families: `TS2322`, `TS2345`, `TS2416`, missing/excess
  property diagnostics, weak type diagnostics, rendered-type mismatch
  fingerprints, source-text diagnostic branches, and accepted-regression
  strictness.
- Architecture cleanup metric: counts of diagnostic decisions based on source
  text, rendered type strings, fixture names, and accepted-regression entries
  should trend down.
- First live command: inspect owned PRs, then run the dashboard command above
  and identify the highest-risk strictness or diagnostic debt item.
- Next concrete step: open or update a draft PR that removes one diagnostic
  shortcut or accepted-regression entry through a structural rule with focused
  tests.

## Existing Work To Inspect First

- Live `agent:M1-A` PRs from `scripts/agents/list-owned-work.sh M1-A`.
- `scripts/conformance/conformance-accepted-regressions.txt`.
- Open issues labelled `accepted-regression`, `bug`, `false-positive`, and
  `false-negative`.
- Recent merged PRs that changed diagnostic rendering, relation diagnostics,
  query boundaries, or conformance fingerprints.

## Non-Overlap Rules

- Do not change solver relation policy, evaluation behavior, or cache keys
  unless the PR is explicitly stacked with M4-B or M4-Opus.
- Do not add new source-text, display-string, fixture-name, or conformance
  test-name special cases.
- If the diagnostic mismatch needs a solver answer or relation policy, hand off
  to M4-B or M4-Opus with a signed comment.
- If the issue is checker routing rather than diagnostic content, coordinate
  with M1-B.

## Verification

- Prefer focused checker tests and narrow conformance queries that answer the
  changed diagnostic path.
- Use `cargo nextest run -p tsz_checker -- <test-filter>` for Rust tests.
- Use `python3 scripts/conformance/query-conformance.py --dashboard` before
  making conformance claims.
- Do not run full conformance locally unless explicitly asked.
