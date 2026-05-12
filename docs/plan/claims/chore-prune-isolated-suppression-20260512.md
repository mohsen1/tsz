# chore(conformance): prune isolated suppression debt

- **Date**: 2026-05-12
- **Branch**: `chore/prune-isolated-suppression-20260512`
- **PR**: #5794
- **Status**: ready
- **Workstream**: conformance debt cleanup

## Intent

Remove the dead `isolatedDeclarationErrorsObjects` production-suppression debt
pattern now that the refreshed conformance snapshot shows only
`recursiveConditionalTypes` and `mixinAccessModifiers` as known failures. This
keeps the suppression allowlist aligned with live debt without changing checker
behavior.

## Files Touched

- `crates/conformance/src/runner.rs`
- `docs/plan/claims/chore-prune-isolated-suppression-20260512.md`

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build --profile dist-fast -p tsz-conformance`
- `scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run --filter isolatedDeclarationErrorsObjects --workers 4` (1/1 passed; skipped 0; known failures 0)
