# fix(checker): honor strict built-in iterator returns

- **Date**: 2026-05-05
- **Branch**: `fix/checker-strict-builtin-iterator-return`
- **PR**: https://github.com/mohsen1/tsz/pull/3186
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the missing `TS2322` diagnostics in
`TypeScript/tests/cases/compiler/iterableTReturnTNext.ts`. Current tsz on
`origin/main` already emits the expected `TS2416` and `TS5024`, but it misses
assignability failures where `MapIterator<T>.next()` should expose
`IteratorResult<T, undefined>` under `strictBuiltinIteratorReturn`.

## Files Touched

- `docs/plan/claims/fix-checker-strict-builtin-iterator-return.md`
- `crates/tsz-checker/src/state/type_analysis/computed/builtin_iterator_return_alias.rs`
- `crates/tsz-checker/src/types/queries/lib_resolution.rs`
- `crates/tsz-checker/tests/lib_resolution_identity_tests.rs`
- `crates/tsz-cli/src/driver/check.rs`

## Verification

- `cargo test --target-dir target-codex -p tsz-cli cloned_checker_libs_preserve_strict_builtin_iterator_return -- --nocapture`
- `cargo test --target-dir target-codex -p tsz-checker builtin_iterator_return -- --nocapture`
- `tsz-conformance --filter iterableTReturnTNext` (patched `target-codex/dist-fast/tsz`)
- `tsz-conformance --max 200` (patched `target-codex/dist-fast/tsz`)
