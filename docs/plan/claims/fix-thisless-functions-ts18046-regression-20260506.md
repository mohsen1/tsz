# fix(checker): remove thisless contextual callback TS18046 regression

- **Date**: 2026-05-06
- **Branch**: `fix/thisless-functions-ts18046-regression-20260506-165500`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/thislessFunctionsNotContextSensitive1.ts`.
The current conformance picker reports one extra TS18046 even though the target
was previously fixed by PR #2759. This PR will re-run the current fixture,
identify the remaining or regressed contextual-callback inference path, and fix
the root cause without a target-specific suppression.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-cli -E 'test(compile_project_nested_thisless_module_state_avoids_ts18046)'`
- `./scripts/conformance/conformance.sh run --filter "thislessFunctionsNotContextSensitive1" --verbose`
