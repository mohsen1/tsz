# Preserve literal-union return inference from default parameters

- **Date**: 2026-05-13
- **Branch**: `fix/literal-union-default-param-return-20260513`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance

## Intent

Address #6127, where a function returning a parameter declared as a string literal union with a default initializer is inferred as `string` instead of the declared literal union. The fix should preserve the parameter's declared receiver type through return inference without broadly disabling ordinary literal widening.

## Files Touched

- `crates/tsz-checker/src/types/utilities/return_type.rs`
- `crates/tsz-checker/tests/tuple_type_assertion_inference_tests.rs`

## Verification

- `cargo test -p tsz-checker --test tuple_type_assertion_inference_tests -- --nocapture` - passed, 5 tests.
- `cargo fmt --all -- --check` - passed.
- `git diff --check` - passed.
- `cargo run -q -p tsz-cli --bin tsz -- --noEmit --strict /tmp/tsz-6127.ts 2>&1 | head -80` - passed, no diagnostics.
