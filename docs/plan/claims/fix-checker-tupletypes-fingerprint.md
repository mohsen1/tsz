# fix(checker): align tupleTypes assignment fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-tupletypes-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06

## Intent

Fix the current conformance failure
`TypeScript/tests/cases/compiler/tupleTypes.ts`. The diagnostic codes already
match TypeScript (`TS2322`, `TS2403`, `TS2454`, `TS2493`, `TS2540`), but the
fingerprints drift for tuple assignment display: `tsz` reports `Type 'B' is
not assignable to type '[number, string]'` at line 15 instead of TypeScript's
`Type '[number]' is not assignable to type '[number, string]'`, and emits an
extra optional tuple length assignment fingerprint at line 65.

## Files Touched

- `docs/plan/claims/fix-checker-tupletypes-fingerprint.md`
- `crates/tsz-checker` / `crates/tsz-solver` tuple assignment diagnostic path (to be narrowed during investigation)
- owning-crate Rust regression test

## Verification

- `/var/tmp/tsz-dist-refresh-3520/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz-main-worktree/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /var/tmp/tsz-dist-refresh-3520/dist-fast/tsz --server-binary /var/tmp/tsz-dist-refresh-3520/dist-fast/tsz-server --workers 1 --filter tupleTypes --print-test --verbose --print-fingerprints --print-test-files` (current baseline: fingerprint-only failure)
- targeted Rust regression test after fix
- targeted conformance rerun for `tupleTypes`
