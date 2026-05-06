# fix(checker): realign jsxElementType fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/jsx-element-type-regression-20260506-183000`
- **PR**: #4209
- **Status**: PR opened
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/jsxElementType.tsx`.
The current canonical picker reports a fingerprint-only mismatch with the
expected diagnostic code set still present (`TS2304`, `TS2322`, `TS2339`,
`TS2741`, `TS2769`, `TS2786`). PR #3200 previously fixed this fixture and was
merged on 2026-05-05, so this slice will identify the current regression and
realign the JSX element-type diagnostics without changing the diagnostic code
set.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/diagnostics.rs`
- `crates/tsz-checker/src/checkers/jsx/props/validation.rs`
- `crates/tsz-checker/src/checkers/jsx/tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib -E 'test(jsx_library_managed_attributes_function_variable_display_uses_param_props)'`
- `./scripts/conformance/conformance.sh run --filter "jsxElementType" --verbose`
- Pre-commit hook: clippy, wasm rustc warnings gate, architecture guardrails,
  and 15,899 nextest tests.
