# [WIP] fix(checker): preserve JSDoc chained-assignment types

- **Date**: 2026-05-05
- **Branch**: `fix/jsdoc-chained-assignment-types`
- **PR**: #2787
- **Status**: claim
- **Workstream**: 1 (Conformance - JSDoc chained-assignment diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/jsdoc/jsdocTypeFromChainedAssignment.ts`,
a fingerprint-only failure with matching `TS2322`, `TS2339`, and `TS2345`
codes. The live mismatch shows `tsz` loses the chained prototype member type
for `a.z(...)` and renders the static chained-assignment function's `this`
type as `g` instead of `typeof A`.

This PR will root-cause the chained-assignment JSDoc/type propagation path and
align the resulting diagnostics with `tsc`, with a focused checker regression
test for the invariant.

## Files Touched

- TBD after implementation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "jsdocTypeFromChainedAssignment" --verbose` (currently fingerprint-only on `origin/main`)
