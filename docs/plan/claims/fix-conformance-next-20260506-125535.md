# fix(checker): align rest argument call fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-125535`
- **PR**: #4078
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/functionCall10.ts`.

`tsc` and tsz already agree on the diagnostic code (`TS2345`) for this
rest-parameter call fixture, but the conformance fingerprints differ. This
slice will diagnose whether the drift is argument diagnostic anchoring,
message rendering, or rest-parameter expected-type display, then align the
fingerprint without changing the intended diagnostic set.

## Files Touched

- TBD

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-125535 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "functionCall10" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases`

## Abandonment Note

The verbose run showed the same sibling-literal call diagnostic display bug
already covered by open PR #4023 (`fix(checker): keep declared call parameter
display`): tsz reports `"bar"` -> `1` where tsc reports `string` -> `number`.
This claim is abandoned to avoid duplicating that in-flight fix.
