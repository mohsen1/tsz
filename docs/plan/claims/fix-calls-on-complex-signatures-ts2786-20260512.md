# Claim: callsOnComplexSignatures TS2786 regression

- **Date**: 2026-05-12
- **Branch**: `fix/calls-on-complex-signatures-ts2786-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance regression)

## Intent

Current `main` regressed `TypeScript/tests/cases/compiler/callsOnComplexSignatures.tsx`: tsc expects no diagnostics, while tsz emits an extra TS2786 for the JSX tag-name union case in `test5`.

Fix the JSX component validity path so valid `React.ComponentType<P1> | React.ComponentType<P2>` component variables do not report TS2786, while preserving the recently fixed TS2786 diagnostics for invalid union component return types.

## Files Touched

TBD after investigation.

## Verification

Planned:

- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter callsOnComplexSignatures --verbose`
- focused checker JSX unit tests
- `cargo fmt --all --check`
- `git diff --check`
