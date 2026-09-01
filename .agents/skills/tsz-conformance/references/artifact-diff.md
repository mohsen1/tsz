# Artifact Delta Contract

Use this reference for emit, conformance, fourslash, or project artifact
comparisons during the rewrite.

## Stable Identity First

Index both artifacts by their canonical per-row key. Reject duplicates and
report additions/removals before interpreting a status transition. Report the
complete status matrix, but do not stop there: compare ordered diagnostic
identities and product bytes for every changed row.

A diagonal matrix or unchanged pass total can hide fabricated diagnostics on a
row that remains incomplete or failing. Classify payload changes as additions,
removals, reorders, or replacements, and isolate timeout/crash scheduling churn
from deterministic semantic changes.

## Emit Direction Gate

For emit artifacts, `TSZ_NONZERO_OUTCOME` means the exact TypeScript 7 oracle
invocation was clean. Its diagnostic sequence may shrink, but it must not gain
or reorder codes relative to the immediate baseline. Run:

```bash
python3 scripts/ci/check-emit-regression-set.py \
  --baseline <prior-emit-detail.json> <current-emit-detail.json>
```

The gate checks named status regressions and ordered code-level growth on those
oracle-clean outcomes. The emit schema stores codes, not complete diagnostic
identities, so this command is necessary but not sufficient for semantic work.
CI uses `scripts/emit/rewrite-regression-baseline.json`, rejects missing or
duplicate stable keys, validates oracle provenance, and compares it to every
reachable committed floor. Never refresh that file to accept a regression.

The full emit direction job runs on the nightly/manual heavy lane, not on each
PR event. Before handoff, attach a local or `workflow_dispatch` artifact audit;
a future nightly failure is not a substitute for pre-merge evidence.

## Oracle Reconciliation

For every added or removed semantic diagnostic, verify the exact code,
normalized path, UTF-16 span, category, message chain, and related information
against the pinned oracle for the exact authored options. New oracle-absent or
removed oracle-required diagnostics block handoff even when the status remains
`fail`, `incomplete`, or `unsupported`.

Record the stable key-set result, status matrix, diagnostic additions/removals,
product changes, artifact hashes, binary/source provenance, and oracle evidence
in the PR body.
