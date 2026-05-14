# fix(checker): report TS1225 for invalid type predicate targets

- **Date**: 2026-05-13
- **Branch**: `codex/type-predicate-ts1225-6606-20260513`
- **PR**: #6625
- **Status**: ready
- **Workstream**: conformance / checker diagnostic

## Intent

Fix #6606 so a type predicate return type must name a function parameter.
`T is infer U` in a conditional function type should report TS1225 when `T` is
a type parameter rather than a parameter name.

## Files Touched

- `crates/tsz-checker/src/checkers/signature_builder.rs`
- `crates/tsz-checker/src/tests/assertion_type_predicate_diagnostics_tests.rs`
- `docs/plan/claims/codex-type-predicate-ts1225-6606-20260513.md`

## Verification

- `cargo test -p tsz-checker type_predicate_target_must_name_function_parameter -- --nocapture` (1 passed)
- `cargo fmt --all --check`
- `cargo test -p tsz-checker assertion_type_predicate_diagnostics_tests --lib -- --nocapture` (18 passed)
