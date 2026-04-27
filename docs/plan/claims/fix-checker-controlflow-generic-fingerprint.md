# fix(checker): align controlFlowGenericTypes diagnostic fingerprints

- **Date**: 2026-04-27
- **Branch**: `fix/checker-controlflow-generic-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`controlFlowGenericTypes.ts`, where tsz emits the same diagnostic codes as
tsc but differs in message text or anchoring. The change will follow the
existing checker/solver diagnostic boundaries and add a focused regression
test for the narrowed behavior.

## Files Touched

- `crates/tsz-checker/src/**` or `crates/tsz-solver/src/**` once the
  diagnostic root cause is isolated
- `crates/tsz-checker/tests/**` or `crates/tsz-solver/tests/**` for the
  regression lock
- `reference/deepwiki/**` for TypeScript and tsgo implementation notes

## Verification

- Pending: `./scripts/conformance/conformance.sh run --filter "controlFlowGenericTypes" --verbose`
