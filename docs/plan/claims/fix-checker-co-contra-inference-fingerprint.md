# [WIP] fix(checker): align co/contravariant inference diagnostics

- **Date**: 2026-04-29
- **Timestamp**: 2026-04-29 21:28:26 UTC
- **Branch**: `fix/checker-co-contra-inference-fingerprint`
- **PR**: #1827
- **Status**: claim
- **Workstream**: 1 - Diagnostic Conformance And Fingerprints

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29. The target
`TypeScript/tests/cases/compiler/coAndContraVariantInferences3.ts` is
fingerprint-only: both `tsc` and `tsz` emit `TS2344` and `TS2430`, but the
diagnostic fingerprints differ. This PR will diagnose whether the gap is type
display, diagnostic anchoring, or duplicate-count/order policy, then fix the
owning checker/solver boundary with a focused Rust regression test.

## Files Touched

- `docs/plan/claims/fix-checker-co-contra-inference-fingerprint.md`

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "coAndContraVariantInferences3" --verbose`
- Planned: focused Rust regression test in the owning crate
- Planned: affected crate `cargo nextest run`
