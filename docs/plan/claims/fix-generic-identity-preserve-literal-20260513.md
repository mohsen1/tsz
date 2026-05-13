# Preserve literal inference through generic identity calls

- **Date**: 2026-05-13
- **Branch**: `fix/generic-identity-preserve-literal-20260513`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance

## Intent

Address #6126, where a generic identity call such as `identity("test")` widens the inferred `T` to `string`, causing a false TS2322 when assigning the result to `"test"`.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/normalization.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo test -p tsz-checker --test generic_call_inference_tests generic_identity_preserves_single_literal_argument -- --nocapture` - passed.
- `cargo test -p tsz-checker --test generic_call_inference_tests return_context_infers_type_argument_from_variable_annotation -- --nocapture` - passed.
- `cargo test -p tsz-checker --test generic_call_inference_tests -- --nocapture` - passed, 133 tests.
- `cargo run -q -p tsz-cli --bin tsz -- --noEmit --strict /tmp/tsz-6126.ts 2>&1 | head -80` - passed, no diagnostics.
- `cargo fmt --all -- --check` - passed.
- `git diff --check` - passed.
