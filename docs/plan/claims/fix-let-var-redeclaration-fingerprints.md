# [WIP] fix(checker): align let/var redeclaration fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/let-var-redeclaration-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/3295
- **Status**: implemented
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/letAndVarRedeclaration.ts`, where `tsc`
and `tsz` emit the same diagnostic codes (`TS2300`, `TS2451`) but the
fingerprints differ.

## Files Touched

- `docs/plan/claims/fix-let-var-redeclaration-fingerprints.md`
- `crates/tsz-checker/src/state/variable_checking/core_tests.rs`
- `crates/tsz-checker/src/state/variable_checking/variable_helpers/core.rs`

## Outcome

Function-scope `let`/`var` redeclaration fallback diagnostics now stand down
when the same effective scope also has a function declaration with that name.
In that three-way conflict, duplicate identifier checking emits the TSC-style
`TS2300` diagnostics on all declarations, and the fallback no longer adds
extra `TS2451` fingerprints.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "letAndVarRedeclaration" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`

## Verification Results

- `cargo fmt --check`
- `CARGO_TARGET_DIR=target-codex CARGO_INCREMENTAL=0 cargo nextest run --target-dir target-codex -p tsz-checker --lib function_scope_let_var_function_conflict_uses_duplicate_identifier_only function_scope_still_emits_ts2451_not_ts2481`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --filter 'compiler/letAndVarRedeclaration.ts' --verbose --print-fingerprints --write-diff-artifacts --diff-artifacts-dir artifacts/conformance/let-var-redecl --workers 2 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
- `./target-codex/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary ./target-codex/dist-fast/tsz --max 200 --workers 4 --max-worker-rss-mb 1024 --max-compilations-per-worker 50`
