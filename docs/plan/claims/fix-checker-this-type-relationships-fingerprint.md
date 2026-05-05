# fix(checker): align this-type relationship fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-this-type-relationships-fingerprint`
- **PR**: #3207
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/thisType/typeRelationships.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2403`, and `TS2739`), so this PR aligns the reported
diagnostic source spans and rendered type fingerprints for this-type
relationship errors.

## Files Touched

- `docs/plan/claims/fix-checker-this-type-relationships-fingerprint.md`
- `crates/tsz-checker/Cargo.toml`
- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/src/error_reporter/render_failure.rs`
- `crates/tsz-checker/src/state/state.rs`
- `crates/tsz-checker/src/types/class_type/core.rs`
- `crates/tsz-checker/src/types/computation/array_literal.rs`
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/tests/this_type_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker --test this_type_tests --hide-progress-bar`
- `cargo check -p tsz-checker -p tsz-solver`
- `cargo fmt --all --check`
- `git diff --check`
- `./scripts/conformance/conformance.sh run --filter "types/thisType/typeRelationships.ts" --verbose`
- `cargo nextest run --profile precommit --no-tests=pass --package tsz-common --package tsz-scanner --package tsz-parser --package tsz-binder --package tsz-solver --package tsz-checker --package tsz-lowering --package tsz-emitter --package tsz-lsp --package tsz-core -E 'package(tsz-common) | package(tsz-scanner) | package(tsz-parser) | package(tsz-binder) | package(tsz-solver) | package(tsz-checker) | package(tsz-lowering) | package(tsz-emitter) | package(tsz-lsp) | package(tsz-core)'`
