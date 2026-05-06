# [WIP] fix(checker): align infer type predicate fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/infer-type-predicates-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 04:21:08 UTC

## Intent

Fix the picked `TypeScript/tests/cases/compiler/inferTypePredicates.ts`
fingerprint-only conformance failure. Current `origin/main` emits the same
diagnostic codes as tsc but misses inferred type-predicate narrowing in several
cases, leaving extra broad assignment diagnostics and one mismatched `Date`
diagnostic surface.

## Files Touched

- `docs/plan/claims/fix-infer-type-predicates-fingerprints.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- targeted owning-crate `cargo nextest run` regression test
- targeted conformance rerun for `inferTypePredicates`
