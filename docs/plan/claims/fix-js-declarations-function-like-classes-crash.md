# fix(checker): stop crash in JS declarations function-like classes

- **Date**: 2026-05-06
- **Branch**: `fix/js-declarations-function-like-classes-crash`
- **PR**: #3663
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 06:52:00 UTC

## Intent

Fix the current conformance crash in
`TypeScript/tests/cases/conformance/jsdoc/declarations/jsDeclarationsFunctionLikeClasses2.ts`.
The targeted current-main conformance run reports the test as `CRASH`.

## Files Touched

- `docs/plan/claims/fix-js-declarations-function-like-classes-crash.md`
- `crates/tsz-checker/src/jsdoc/resolution/name_resolution.rs`
- `crates/tsz-checker/src/types/computation/complex_js_constructor.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo nextest run --target-dir /Users/mohsen/code/tsz-main-worktree/.target -p tsz-cli checked_js_declaration_emit_self_referential_prototype_method_type_does_not_recurse`
- `/Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz --server-binary /Users/mohsen/code/tsz-main-worktree/.target/dist-fast/tsz-server --workers 1 --filter jsDeclarationsFunctionLikeClasses2 --print-test --verbose --print-fingerprints --print-test-files`
