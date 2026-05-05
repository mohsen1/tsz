# [WIP] fix(checker): align JSX children diagnostic surfaces

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next16`
- **PR**: #3147
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`jsxChildrenIndividualErrorElaborations.tsx`. The diagnostic code set already
matches tsc, but several JSX children diagnostics preserve `Cb` alias displays
where tsc reports the expanded function type or the declared union surface.

## Files Touched

- `crates/tsz-checker/src/...` (expected diagnostic surface/anchor changes)
- `crates/tsz-checker/tests/...` (focused regression coverage)

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/jsxChildrenIndividualErrorElaborations.tsx`.
