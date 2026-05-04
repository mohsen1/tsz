# [WIP] fix(checker): align generic rest parameter diagnostics

- **Date**: 2026-05-04
- **Branch**: `fix/generic-rest-parameters3-fingerprint`
- **PR**: #2732
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only diagnostic divergence in
`genericRestParameters3.ts`. The expected and actual diagnostic codes already
match (`TS2322`, `TS2345`, `TS2554`), so this PR will narrow the checker or
printer behavior that is producing mismatched diagnostic text or anchors for
generic rest parameter calls.

## Files Touched

- TBD after root-cause investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "genericRestParameters3" --verbose`
- `cargo nextest run` for the owning crate tests added or changed by the fix.
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
