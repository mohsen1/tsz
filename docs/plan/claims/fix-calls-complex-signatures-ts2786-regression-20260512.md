# fix(checker): restore callsOnComplexSignatures JSX union validity

- **Date**: 2026-05-12
- **Branch**: `fix/calls-complex-signatures-ts2786-regression-20260512`
- **Base**: `main`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Restore the `callsOnComplexSignatures.tsx` conformance pass on current `main`.
The earlier fix in PR #5679 merged, but a later change reintroduced an extra
TS2786 for a JSX tag-name union case while tsc expects no diagnostics.

## Scope

- Reproduce the focused `callsOnComplexSignatures` delta.
- Fix the smallest JSX component validity regression without weakening invalid
  component diagnostics such as `jsxComponentTypeErrors`.
- Add or update focused regression coverage in `tsz-checker` if the root cause is
  isolated.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "callsOnComplexSignatures" --verbose`
- Relevant JSX regression tests in `tsz-checker`
- Guard conformance for `jsxComponentTypeErrors`
- `cargo fmt --all`
- Pre-commit or equivalent direct-crate validation before marking ready

## Progress

- Claim created.
