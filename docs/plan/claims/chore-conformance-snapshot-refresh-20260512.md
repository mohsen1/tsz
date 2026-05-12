# chore(conformance): refresh stale snapshot artifacts

- **Date**: 2026-05-12
- **Branch**: `chore/conformance-snapshot-refresh-20260512`
- **PR**: TBD
- **Status**: claim
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

- Pending: `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot`
