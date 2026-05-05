# fix(parser): align parameter syntax error fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/parameters-syntax-error-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/3323
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/parametersSyntaxErrorNoCrash1.ts`, where
`tsc` and `tsz` both emit `TS1005` but the diagnostic fingerprint differs for
a malformed parameter type annotation.

## Files Touched

- `docs/plan/claims/fix-parameters-syntax-error-fingerprint.md`
- `crates/tsz-parser/src/parser/state_expressions_literals.rs`
- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-parser --lib test_parameter_list_stray_colon_recovers_through_object_binding_tail`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-parser --lib`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --filter 'compiler/parametersSyntaxErrorNoCrash1.ts' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/parameters-syntax --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --max 200 --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
