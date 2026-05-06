# fix(checker): suppress contextual function object intersection cascades

- **Date**: 2026-05-06
- **Branch**: `codex/contextual-function-object-intersection-20260506`
- **PR**: https://github.com/mohsen1/tsz/pull/3707
- **Status**: implemented
- **Workstream**: 1 (Conformance)

## Intent

Fix the current `TypeScript/tests/cases/compiler/contextualTypeFunctionObjectPropertyIntersection.ts`
conformance failure. The filtered conformance run on current `origin/main`
emits two extra diagnostics, `TS2322` at the object literal and cascading
`TS2345` at the call, while tsc accepts the contextual callback object.

The expected impact is a one-test conformance pass-rate increase by preserving
the intended contextual typing through the function-object intersection case
without weakening unrelated object-literal excess-property diagnostics.

## Files Touched

- `docs/plan/claims/codex-contextual-function-object-intersection-20260506.md`
- `crates/tsz-solver/src/objects/collect.rs`
- `crates/tsz-solver/src/relations/subtype/helpers.rs`
- `crates/tsz-checker/tests/contextual_typing_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests test_deferred_mapped_intersection_preserves_contextual_property_types -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests test_contextual_function_object_property_intersection_sequence -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/contextual-3707 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build -p tsz-cli --bin tsz`
- Filtered conformance for `compiler/contextualTypeFunctionObjectPropertyIntersection.ts`: `1/1 passed (100.0%)`
- `git diff --check`
- Debug-print scan found only existing TypeScript `console.log` fixture text.
