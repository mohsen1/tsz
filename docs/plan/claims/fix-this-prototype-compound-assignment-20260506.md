# fix(checker): suppress checked js prototype compound false positive

- **Date**: 2026-05-06
- **Branch**: `fix/this-prototype-compound-assignment-20260506-234200`
- **PR**: #4324
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Target
`TypeScript/tests/cases/conformance/jsdoc/thisPrototypeMethodCompoundAssignmentJs.ts`.
The canonical picker reports a false-positive diagnostic: expected no
diagnostics, actual `TS2531`. This slice will identify why checked JavaScript
prototype-method compound assignment treats the receiver as nullable, and fix
the owning checker path without suppressing unrelated nullability diagnostics.

## Files Touched

- `crates/tsz-checker/src/types/computation/call/tail_helpers.rs`
- `crates/tsz-checker/tests/js_constructor_property_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test js_constructor_property_tests checked_js_prototype`
- `./scripts/conformance/conformance.sh run --filter "thisPrototypeMethodCompoundAssignmentJs" --verbose`
- `./scripts/conformance/conformance.sh run --filter "thisPrototypeMethodCompoundAssignment" --verbose`
- Pre-commit hook: clippy, wasm rustc warnings gate, architecture guardrails, affected-crate nextest (16052 passed, 57 skipped).
