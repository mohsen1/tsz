# fix(checker): align builtin Iterator abstract compatibility

- **Date**: 2026-04-29
- **Branch**: `fix/checker-builtin-iterator-abstract-compat`
- **PR**: #1799
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`TypeScript/tests/cases/compiler/builtinIterator.ts` previously reported an
extra construct-signature diagnostic and missed the expected abstract-class,
abstract-member, override, and Iterator/Iterable compatibility diagnostics.
This PR aligns TSZ with tsc for the builtin `Iterator` conformance case.

## Files Touched

- `crates/tsz-checker/src/types/queries/lib_resolution.rs`
- `crates/tsz-checker/src/query_boundaries/common.rs`
- `crates/tsz-checker/src/types/interface_type.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/checkers/call_checker/mod.rs`
- `crates/tsz-checker/src/classes/class_abstract_checker.rs`
- `crates/tsz-checker/src/classes/class_checker.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/tests/lib_resolution_identity_tests.rs`
- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-solver/src/relations/subtype/rules/generics.rs`
- `crates/tsz-solver/src/type_queries/core.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo fmt --check`
- `cargo clippy --package tsz-checker --all-targets -- -D warnings`
- `cargo nextest run --package tsz-checker --test lib_resolution_identity_tests test_builtin_iterator_constructor_uses_scoped_abstract_typeof_alias test_builtin_iterator_protocol_uses_scoped_defaults_in_errors`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter builtinIterator --verbose` (2/2 passed)
