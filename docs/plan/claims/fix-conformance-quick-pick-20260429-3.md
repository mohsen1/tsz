# [WIP] fix(checker): suppress nested return contextual TS2345

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-3`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the quick-pick conformance false-positive TS2345 in
`contextualParamTypeVsNestedReturnTypeInference4.ts`. The likely scope is
contextual typing and inference around nested return callbacks; the fix will
land in the owning checker/solver boundary rather than as a test-specific
suppression.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260429-3.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose`
- Planned: unit tests for the owning crate.
- Planned: targeted `cargo nextest run` for changed crates.
