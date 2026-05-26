# Agent Goal: M1-C

AgentName: M1-C
Computer: M1
Session: C
GitHub label: `agent:M1-C`

## Mission

Own conformance strictness and diagnostic hardcoding burn-down. Exact
conformance must stay `12,582 / 12,582`, and
`scripts/conformance/conformance-accepted-regressions.txt` must trend to zero.

## Start Every Cycle

```bash
git fetch origin main
scripts/agents/show-goal.sh M1-C
scripts/agents/disk-preflight.sh M1-C
scripts/agents/list-owned-work.sh M1-C
python3 scripts/conformance/query-conformance.py --dashboard
```

## Current Assignment

- Primary gate: conformance strictness, not just rounded `100.0%`.
- Bug families: accepted-regression paths, fingerprint-only drift,
  wrong-position diagnostics, rendered-type/source-text/file-name decisions,
  and hardcoded fingerprint filters.
- Architecture cleanup metric: post-check `rewrite_*_fingerprints`,
  `source_text.contains`, rendered-type decision, and accepted-regression
  counts must trend down.
- First live command: read the dashboard and accepted-regression list, then
  find or create owner issues for any listed path lacking current ownership.
- Next concrete step: pick one accepted-regression or hardcoded diagnostic
  decision, identify the structural fact it approximates, and replace it with a
  solver/query-boundary or syntax-gate fact.

## Existing Work To Inspect First

- `scripts/conformance/conformance-accepted-regressions.txt`.
- Issues `#7596`, `#10234`, `#10164`, `#10149`, `#9906`, and `#8286`.
- `docs/architecture/WELL_KNOWN_NAME_REFERENCES.md`.
- Recent diagnostic display and fingerprint-filter PRs.

## Non-Overlap Rules

- Do not drive decisions from `format_type_diagnostic`, `contains`,
  `starts_with`, regexes, file names, or conformance test names.
- Do not widen or mutate semantic types to make messages pretty. Presentation
  belongs in display policy.
- If the needed fact is semantic, expose it through solver/query boundaries.
- Accepted-regression list edits need current CI or focused artifact evidence.

## Verification

- Use offline conformance query tools first.
- Add focused checker/solver tests for behavior changes, with renamed binders
  when names are involved.
- Use narrow conformance filters only when offline data is insufficient.
- Do not run full conformance locally.
