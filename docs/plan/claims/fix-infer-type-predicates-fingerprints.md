# [WIP] fix(checker): align infer type predicate fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/infer-type-predicates-fingerprints`
- **PR**: #3691
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 04:21:08 UTC

## Intent

Fix the picked `TypeScript/tests/cases/compiler/inferTypePredicates.ts`
fingerprint-only conformance failure. Current `origin/main` emits the same
diagnostic codes as tsc but misses inferred type-predicate narrowing in several
cases, leaving extra broad assignment diagnostics and one mismatched `Date`
diagnostic surface.

## Files Touched

- `docs/plan/claims/fix-infer-type-predicates-fingerprints.md`
- `crates/tsz-checker/src/flow/control_flow/type_guards.rs`
- `crates/tsz-checker/src/checkers/signature_builder.rs`
- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-checker/src/types/computation/identifier_flow.rs`
- `crates/tsz-checker/src/error_reporter/render_failure.rs`
- `crates/tsz-checker/src/error_reporter/render_failure/type_mismatch.rs`
- `crates/tsz-solver/src/relations/subtype/explain.rs`
- `crates/tsz-solver/src/visitors/visitor_predicates.rs`
- `crates/conformance/src/runner.rs`
- `crates/tsz-checker/tests/control_flow_type_guard_tests.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`

## Verification

- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-infer-predicates -p tsz-checker --test control_flow_type_guard_tests inferred_type_predicate -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test --target-dir /var/tmp/tsz-target-infer-predicates -p tsz-checker --test ts2322_tests test_object_source_missing_date_properties_not_downgraded_to_ts2322 -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance --target-dir /var/tmp/tsz-target-infer-predicates`
- `/var/tmp/tsz-target-infer-predicates/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /var/tmp/tsz-target-infer-predicates/dist-fast/tsz --server-binary /var/tmp/tsz-target-infer-predicates/dist-fast/tsz-server --workers 1 --filter inferTypePredicates --print-test --verbose --print-fingerprints --print-test-files`

Targeted conformance result: `FINAL RESULTS: 1/1 passed (100.0%)`, known failures `0`.

Note: `cargo nextest run --target-dir /var/tmp/tsz-target-infer-predicates -p tsz-checker inferred_type_predicate`
could not complete in this workspace because nextest linked all checker test binaries
before applying the filter and hit `errno=28` (no space left on device). The focused
`cargo test --test control_flow_type_guard_tests inferred_type_predicate` command
ran the same inferred-predicate regression tests without linking unrelated binaries.
