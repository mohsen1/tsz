# [WIP] fix(checker): align generic optional fold fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/generic-optional-fold-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/3263
- **Status**: implemented
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/genericFunctionsWithOptionalParameters2.ts`,
where `tsc` and `tsz` emit the same diagnostic codes (`TS2345`, `TS2554`) but
the fingerprints differ.

## Files Touched

- `docs/plan/claims/fix-generic-optional-fold-fingerprints.md`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/src/error_reporter/type_display_policy.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`

## Outcome

Canonical `Array<T>` diagnostic surfaces now render as `T[]` for call
parameter displays, including the preserved call-result fallback used by
generic call inference. Generic constraint displays still preserve explicit
`Array<T>` syntax.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "genericFunctionsWithOptionalParameters2" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`

## Verification Results

- `cargo fmt --check`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-checker --lib generic_optional_array_parameter_diagnostic_uses_array_shorthand`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-solver --lib format_function_type_param_with_non_primitive_array_constraint_uses_generic_form format_function_type_param_with_array_application_constraint_preserves_generic_form`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --filter 'compiler/genericFunctionsWithOptionalParameters2.ts' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/generic-fold --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --max 200 --workers 4 --max-worker-rss-mb 1024 --max-compilations-per-worker 50`
