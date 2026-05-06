# fix(checker): suppress infer conditional mapped member TS2344

- **Date**: 2026-05-06
- **Branch**: `fix/checker-infer-conditional-constraint-mapped-member`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 02:07:12 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/compiler/inferConditionalConstraintMappedMember.ts`.
The picker reports that TypeScript emits no diagnostics, while `tsz` emits an
extra `TS2344`.

## Files Touched

- `docs/plan/claims/fix-checker-infer-conditional-constraint-mapped-member.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- `scripts/session/quick-pick.sh`
- `./scripts/conformance/conformance.sh run --filter "inferConditionalConstraintMappedMember" --verbose`
- targeted owning-crate `cargo nextest run` regression test
- targeted conformance rerun for `inferConditionalConstraintMappedMember`
