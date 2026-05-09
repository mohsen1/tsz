# fix(checker): align unknown control-flow fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-unknown-control-flow-fingerprints`
- **PR**: #3066
- **Status**: implemented
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/types/unknown/unknownControlFlow.ts`.
Previous merged slices handled unknown-like union assignability, explicit
unknown-intersection TS2367 emission, and keyof display; this claim is scoped
to the current picker result on `origin/main`, where the diagnostic code set
already matches `tsc` (`TS2322`, `TS2345`, `TS2367`, `TS2536`) but one or more
fingerprints still differ.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/diagnostics/format/tests.rs`
- `crates/tsz-solver/tests/evaluate_tests.rs`
- `crates/tsz-solver/tests/keyof_comprehensive_tests.rs`
- `crates/tsz-checker/src/types/utilities/enum_utils.rs`
- `crates/tsz-checker/src/types/computation/binary_tests.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/accessors.rs`
- `scripts/arch/arch_guard.py`
- `crates/tsz-checker/src/tests/architecture_contract_tests.rs`
- `docs/plan/ROADMAP.md`

## Verification

- `cargo fmt --all`
- `cargo check --package tsz-checker --package tsz-solver`
- `cargo test -p tsz-checker ts2367_for_object_or_null_constrained_intersection_compared_to_primitive -- --nocapture`
- `cargo test -p tsz-checker test_unknown_control_flow_generic_keyspace_and_overlap_regression -- --nocapture`
- `cargo test -p tsz-solver keyof_never -- --nocapture`
- `cargo test -p tsz-solver format_keyof_nullish_collapses_to_never -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "unknownControlFlow" --verbose` (1/1 pass)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 pass)
- pre-commit hook
