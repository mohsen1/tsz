# `perf/master` — perf-plan tracking branch

This branch hosts perf-plan / perf-counter / observability work while `main`
is being driven to 100% TypeScript conformance. See #5085 for the policy.

## Workflow

1. New perf-plan PRs target this branch, **not** `main`.
2. PR titles use the `perf:` prefix.
3. Each sub-PR carries its own validation (benches, attribution counters,
   conformance-safe check against this branch's state).
4. The tracking PR for `perf/master` → `main` stays **draft** until a
   maintainer explicitly greenlights a merge back into `main`.

## What lives here vs. on `main`

- `main`: conformance fixes, conformance snapshot bookkeeping, review fixes
  for conformance PRs.
- `perf/master`: T2.1 lifetime split, T2.2 cross-file query migration,
  perf-counter JSON surface, attribution counter coverage, related
  refactors documented in `docs/plan/PERFORMANCE_PLAN.md`.

## Related

- #5085 — policy issue.
- #5087 — revert of perf-plan churn from `main`.
