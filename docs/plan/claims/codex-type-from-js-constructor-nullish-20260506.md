# fix(solver): accept inferred rest tuples against array constraints

- **Date**: 2026-05-06
- **Branch**: `codex/type-from-js-constructor-nullish-20260506`
- **PR**: #3603
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Fix the `TypeScript/tests/cases/compiler/recursiveTypeAliasWithSpreadConditionalReturnNotCircular.ts`
diagnostic mismatch where tsz emits false-positive `TS2345` diagnostics for
generic rest calls whose inferred tuple argument type must satisfy an
`Array<Option<any>>` constraint.

The expected impact is a one-test conformance pass-rate increase without
changing unrelated generic rest inference behavior.

## Files Touched

- `docs/plan/claims/codex-type-from-js-constructor-nullish-20260506.md`
- `crates/tsz-solver/src/relations/subtype/core.rs`
- `crates/tsz-checker/tests/spread_rest_tests.rs`

## Verification

- `./.target/dist-fast/tsz-conformance --test-dir tmp-conformance-cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter typeFromJSConstructor --workers 1 --verbose --print-fingerprints` — 1/1 passed, confirming the original claim target was stale.
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test spread_rest_tests generic_rest_tuple_inference_satisfies_array_constraint -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `./.target/dist-fast/tsz-conformance --test-dir tmp-conformance-cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter recursiveTypeAliasWithSpreadConditionalReturnNotCircular --workers 1 --verbose --print-fingerprints` — 1/1 passed
