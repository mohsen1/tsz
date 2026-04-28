# fix(checker): validate await thenable this-context

- **Date**: 2026-04-27
- **Branch**: `fix/checker-await-this-context-thenable`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the roadmap quick-pick failure `await_incorrectThisType.ts`.

The primary behavior change adds TS1320 for an `await` operand whose `then` method is only callable for a narrower `this` type. The implementation keeps the behavior in the checker/solver query path for await thenable validation, following tsc's pattern of filtering unusable `then` signatures before extracting the awaited type.

The completed fix also removes the extra diagnostics that blocked the conformance case. Contextual constructor return handling preserves the class type arguments, same-wrapper return-context inference now prefers structured union arms over naked type-variable fallback, and generic method compatibility treats an explicit source `this` parameter as compatible when the target signature has no `this` parameter.

## Files Touched

- `crates/tsz-checker/src/checkers/promise_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/classes/class_implements_checker/core.rs`
- `crates/tsz-checker/src/checkers/signature_builder.rs`
- `crates/tsz-checker/src/error_reporter/assignability_helpers.rs`
- `crates/tsz-checker/src/query_boundaries/class.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/src/types/computation/access_await.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call/mod.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/computation/complex.rs`
- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-solver/src/inference/infer_matching.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-solver/src/operations/generic_call/return_context.rs`
- `crates/tsz-checker/tests/await_thenable_this_context_tests.rs`
- `crates/tsz-checker/Cargo.toml`

## Verification

- `cargo test -p tsz-checker --test await_thenable_this_context_tests` (4 tests pass)
- `cargo test -p tsz-checker --test await_generic_non_promise_no_false_ts2339_tests --test async_return_widening_tests --test await_thenable_this_context_tests` (9 tests pass)
- `cargo fmt --check`
- `./scripts/conformance/conformance.sh run --filter "await_incorrectThisType" --verbose` (1/1 passed)
