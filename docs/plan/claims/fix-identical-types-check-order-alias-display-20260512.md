# fix(checker): preserve alias arguments in identical type check-order diagnostics

- **Date**: 2026-05-12
- **Branch**: `fix/identical-types-check-order-alias-display-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Close the `identicalTypesNoDifferByCheckOrder.ts` fingerprint-only
conformance failure. The current diagnostic expands aliases such as
`SomePropsX` into `Required<Pick<...>> & Omit<...>` inside
`FunctionComponent<T>` source display; tsc preserves the local alias argument
name in the TS2322 message.

## Files Touched

- TBD after investigation; expected checker diagnostic type display path.

## Verification

- Baseline: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter identicalTypesNoDifferByCheckOrder --verbose` (0/1, fingerprint-only)
- Planned: focused checker regression for alias argument display
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter identicalTypesNoDifferByCheckOrder --verbose`
