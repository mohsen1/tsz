---
name: tsz-conformance
description: Triage and maintain TSZ diagnostic conformance. Use when investigating conformance regressions, accepted-regression drift, fingerprint-only failures, issue creation from conformance data, or focused parity fixes that must preserve the conformance gate.
---

# TSZ Conformance

Conformance is a regression gate. Prefer checked-in artifacts and narrow filters;
CI owns broad runs.

## Rules

- Read `AGENTS.md` and `docs/plan/ROADMAP.md` for conformance-affecting work.
- Do not run full conformance locally.
- Treat a failing test as a witness for a structural rule.
- Do not hide regressions with snapshot/allowlist churn.

## Offline First

```bash
python3 scripts/conformance/query-conformance.py --dashboard
python3 scripts/conformance/query-conformance.py --campaigns
python3 scripts/conformance/query-conformance.py --fingerprint-only
python3 scripts/conformance/query-conformance.py --code TS2322
python3 scripts/conformance/query-conformance.py --code TS2322 --paths-only
```

Artifacts: `conformance-detail.json`, `conformance-snapshot.json`,
`conformance-accepted-regressions.txt`, `conformance-shard-weights.json`.

## Focused Run

```bash
./scripts/conformance/conformance.sh run --filter "<name>" --verbose
```

Keep filters precise. Let harnesses rebuild stale binaries when possible.

## Triage

Classify: new/accepted/resolved/fingerprint-only/wrong-code/missing-code/
extra-code/crash/timeout/OOM. Then identify owner: relation, inference,
narrowing, indexed/keyof/mapped/conditional/template, symbol resolution,
diagnostic display, parser recovery, or emit-only.

Before coding, state the structural rule and adjacent cases. Behavior changes
need owning-crate tests.

Accepted-regression drift: verify shard artifacts, update the accepted file
only to match observed failing set, link/file issues for new accepts, and
comment with numbers plus a provenance line.
