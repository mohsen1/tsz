# chore(conformance): refresh snapshot after destructuring tuple fix

- **Date**: 2026-05-06
- **Branch**: `chore/conformance-refresh-20260506-destructuringtuple`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance metrics)
- **Claimed**: 2026-05-06

## Intent

Refresh the committed conformance snapshot after `origin/main` already includes
the merged `destructuringTuple.ts` fix. The canonical picker selected that test
from stale `scripts/conformance/conformance-detail.json`, while a direct
targeted run on `origin/main` reports `FINAL RESULTS: 1/1 passed (100.0%)`.
This PR updates the metrics artifacts so fixed diagnostics leave the failure
pool and the public pass count moves upward.

## Files Touched

- `docs/plan/claims/chore-conformance-refresh-20260506-destructuringtuple.md`
- `scripts/conformance/conformance-detail.json`
- `scripts/conformance/conformance-snapshot.json`

## Verification

- `./scripts/conformance/conformance.sh run --profile dev --filter "destructuringTuple" --verbose --workers 1` (confirmed by direct runner as 1/1 passed before claim)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot`
