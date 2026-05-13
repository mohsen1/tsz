# fix(checker): contextually type default parameter functions

- **Date**: 2026-05-13
- **Branch**: `fix-default-param-contextual-function-7006-6298-20260513`
- **PR**: #6301
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance / contextual typing)

## Intent

Issue #6298 reports a TS7006 false positive for function expressions used as default parameter initializers when the parameter has a function type annotation. The fix should route the parameter annotation as contextual type for the initializer without broadening unrelated callback inference.

After pulling latest `main`, the repro already exits cleanly. This PR records the missing regression to keep the behavior covered.

## Files Touched

- `crates/tsz-cli/tests/tsc_compat_tests.rs`
- `docs/plan/claims/fix-default-param-contextual-function-7006-6298-20260513.md`

## Verification

- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict --pretty false /tmp/issue6298.ts`
- `cargo test -p tsz-cli --test tsc_compat_tests default_parameter_function_initializer_gets_contextual_type -- --nocapture`
- `cargo fmt --all -- --check`
