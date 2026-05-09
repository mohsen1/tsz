# [WIP] fix(checker): align JS constructor void property diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next5`
- **PR**: #2790
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in `assignmentToVoidZero2.ts`.
The picked failure has matching diagnostic codes, but tsz misses the
constructor-assignment TS2339 fingerprint for `this.q = void 0`. The assignment
should not declare a visible JavaScript constructor instance property, and tsc
also reports the later `c.q` access.

## Files Touched

- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/tests/conformance_issues/features/namespace_construct_signature.rs`
- `crates/tsz-checker/tests/js_constructor_property_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/salsa/assignmentToVoidZero2.ts`.
- `cargo nextest run -p tsz-checker --test js_constructor_property_tests`
- `cargo nextest run -p tsz-checker`
- `scripts/conformance/conformance.sh run --filter "assignmentToVoidZero2" --verbose --workers 1 --no-batch`
- `scripts/conformance/conformance.sh run`:
  - `12445/12582` passed, net `+8`
  - `assignmentToVoidZero2.ts` changed `FAIL -> PASS`
  - Reported PASS->FAIL deltas reproduce with this patch stashed and the
    baseline binary rebuilt:
    - `dynamicNames.ts`
    - `noImplicitAnyIndexing.ts`
    - `jsDeclarationsTypeAliases.ts`
    - `typedefTagTypeResolution.ts`
    - `noUncheckedIndexedAccess.ts`
