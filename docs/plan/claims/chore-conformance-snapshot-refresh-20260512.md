# chore(conformance): refresh stale snapshot artifacts

- **Date**: 2026-05-12
- **Branch**: `chore/conformance-snapshot-refresh-20260512`
- **PR**: #5782
- **Status**: ready
- **Workstream**: conformance snapshot accuracy

## Intent

Refresh the committed conformance snapshot artifacts after the latest merged
conformance fixes so the dashboard reports current pass/fail state instead of
stale fingerprint-only failures. This addresses issue #5775 without touching
checker behavior.

## Files Touched

- `scripts/conformance/conformance-snapshot.json`
- `scripts/conformance/conformance-detail.json`
- `scripts/conformance/conformance-baseline.txt`
- `docs/plan/claims/chore-conformance-snapshot-refresh-20260512.md`

## Verification

- `scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh snapshot --workers 16` (12580/12582 passed; 2 known failures)
