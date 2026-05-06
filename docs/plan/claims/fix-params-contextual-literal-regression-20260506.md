# fix(checker): realign params contextual literal diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/params-contextual-literal-regression-20260506-185439`
- **PR**: TBD
- **Status**: implemented; awaiting PR
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized.ts`.
The current conformance run reports a fingerprint-only TS2345 mismatch. PR
#2762 previously aligned this fixture, but tsz now displays the unresolved
mapped parameter target `{ [x in K]?: Lower<T>[] | undefined; }` for the first
argument, while tsc displays the instantiated key-specific targets
`{ y?: number[] | undefined; }` and `{ x?: string[] | undefined; }`.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib -E 'test(mapped_parameter_property_mismatch_displays_instantiated_property_slice)'`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test generic_call_inference_tests -E 'test(mapped_object_key_inference_is_lower_priority_than_direct_key_argument)'`
- `./scripts/conformance/conformance.sh run --filter "paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized" --verbose`
- Pre-commit hook: clippy, wasm rustc warnings gate, architecture guardrails,
  and 15,952 nextest tests.
