---
name: tsz-conformance
description: Triage and maintain TSZ diagnostic conformance. Use when investigating conformance regressions, accepted-regression drift, fingerprint-only failures, issue creation from conformance data, or focused parity fixes that must preserve the conformance gate.
---

# TSZ Conformance

Use this skill to inspect conformance state without turning local work into a
full-suite run. Conformance is a regression gate; use snapshots and targeted
filters first, then let ready-review CI run the broad suite.

## Ground Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` before conformance-affecting work.
- Inspect open PRs/issues for overlapping conformance or checker/solver fixes.
- Do not run full conformance locally. Use narrow filters only.
- Treat the reported test as a witness for a structural rule, not as the scope.
- Do not hide regressions in snapshot or allowlist churn. Fix them, revert them,
  or file a tracking issue when accepting a known runway item.

## Fast Orientation

Prefer offline snapshot data:

```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --code TS2322 --paths-only
```

Use raw artifacts when the query output is too aggregated:

- `scripts/conformance/conformance-detail.json` for per-test expected/actual
  diagnostics and fingerprints.
- `scripts/conformance/conformance-snapshot.json` for aggregate KPIs.
- `scripts/conformance/conformance-accepted-regressions.txt` for the current
  accepted regression runway.
- `scripts/conformance/conformance-shard-weights.json` for shard balancing.

## Focused Reproduction

Run one test or one family only when the offline data is insufficient:

```bash
./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose
```

Keep the filter precise. If the TypeScript submodule or dist binary is stale,
prefer the harness rebuild path over manual broad rebuilds.

## Classification Workflow

1. Determine whether the failure is new, accepted, resolved, fingerprint-only,
   wrong-code, missing-code, extra-code, crash, timeout, or OOM.
2. Identify the owning semantic operation: relation, inference, narrowing,
   indexed access, mapped/conditional/template evaluation, symbol resolution,
   diagnostic display, parser recovery, or emit-only behavior.
3. State the structural rule before coding:
   `When <structural condition>, tsc does X; tsz should do X through <owner>.`
4. Check adjacent cases before choosing a fix: renamed binders, alias wrappers,
   nested forms, generic and concrete forms, positive and negative cases.
5. Add focused owning-crate tests for behavior changes.

## Accepted-Regression Drift

When CI aggregate reports both unlisted regressions and accepted regressions
that no longer fail:

1. Verify shard detail artifacts for the exact commit when available.
2. Update `scripts/conformance/conformance-accepted-regressions.txt` only to
   make the accepted set match the observed failing set.
3. File or link a GitHub issue for every newly accepted regression.
4. Comment on the PR with the aggregate numbers, paths added/removed, and issue
   link. Include the current `AgentName`.

## Issue Shape

For conformance issues, include:

- test path and failure class,
- expected vs actual diagnostic codes or fingerprint class,
- minimized repro when available,
- suspected owner layer,
- adjacent cases that should share the rule,
- CI run or artifact source,
- `AgentName`.
