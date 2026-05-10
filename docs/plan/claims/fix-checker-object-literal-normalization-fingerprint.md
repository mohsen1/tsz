# fix(checker): preserve object literal normalization diagnostics

- **Date**: 2026-05-10
- **Branch**: `fix/object-literal-normalization-2026-05-10`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/objectLiterals/objectLiteralNormalization.ts`.
The checker now preserves normalized object literal source information through
inference and assignability reporting so diagnostics keep the TypeScript
literal display and count behavior.

## Files Touched

- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/assignability_normalized_union.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/src/error_reporter/mod.rs`
- `crates/tsz-checker/src/state/state_checking/mapped_object_literals.rs`
- `crates/tsz-checker/src/types/computation/call_finalize.rs`
- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
- `crates/tsz-checker/tests/object_literal_normalization_tests.rs`
- `crates/tsz-solver/src/inference/infer.rs`
- `crates/tsz-solver/src/inference/infer_resolve.rs`

## Verification

- `cargo test -p tsz-checker --test object_literal_normalization_tests`
- `./scripts/conformance/conformance.sh run --test-dir /tmp/tsz-objectlit-cases --filter objectLiteralNormalization --verbose`
