# fix(solver): keep strict inheritance signatures opaque

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-130614`
- **PR**: #4084
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/conformance/types/typeRelationships/assignmentCompatibility/callSignatureAssignabilityInInheritance6.ts`.

`tsc` and tsz already agree on the diagnostic codes (`TS2430`, `TS2564`),
but the conformance fingerprints differ. This slice will diagnose whether
the drift is interface-heritage diagnostic anchoring, TS2430 message
rendering, or generic call-signature compatibility reporting, then align the
fingerprints without changing the intended diagnostic set.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-solver/tests/integration_tests.rs`
- `crates/tsz-checker/tests/ts2430_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-solver --lib -E 'test(test_strict_member_compat_rejects_outer_type_param_as_generic_signature)'`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib -E 'test(test_outer_type_param_call_property_against_generic_base_errors)'`
- `CARGO_BUILD_JOBS=2 cargo check -p tsz-solver --lib && CARGO_BUILD_JOBS=2 cargo check -p tsz-checker --lib`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "callSignatureAssignabilityInInheritance6" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-solver --lib`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases`

Notes:

- Full `tsz-solver` library nextest passed: 5671 passed, 9 skipped.
- Full `tsz-checker` library nextest passed: 3662 passed, 10 skipped.
- The 200-case conformance smoke is 199/200 with the existing fingerprint-only `anyIndexedAccessArrayNoException.ts` TS2538 column drift. The same focused conformance case fails on detached `origin/main`.
