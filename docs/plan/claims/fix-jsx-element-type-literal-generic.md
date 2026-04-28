# fix(checker): resolve generic JSX.ElementType intrinsic tags

- **Date**: 2026-04-27
- **Branch**: `fix/jsx-element-type-literal-generic`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance failure in `jsxElementTypeLiteralWithGeneric.tsx`, where
TSZ reports `TS2694`/`TS7026` instead of tsc's `TS2339`/`TS2786`. The slice
focuses on JSX namespace/type lookup and unknown intrinsic element diagnostics
for a global `JSX.ElementType<P = any>` type literal that maps over
`JSX.IntrinsicElements`.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs`
- `crates/tsz-checker/src/checkers/jsx/orchestration/component_props.rs`
- `crates/tsz-checker/src/state/type_analysis/core.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Verification

- `cargo test -p tsz-checker --test jsx_component_attribute_tests jsx_element_type_literal_with_generic_merges_global_jsx_exports -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "jsxElementTypeLiteralWithGeneric" --verbose`
