# chore(conformance): prune recursive conditional suppression debt

- **Date**: 2026-05-12
- **Branch**: `fix/recursive-conditional-suppression-20260512`
- **Base**: `main`
- **Workstream**: conformance

## Intent

Remove the stale `recursiveConditionalTypes` production suppression debt entry
after verifying the targeted conformance test now passes without known-failure
handling. This leaves `mixinAccessModifiers` as the only live production
suppression while #5756 tracks that WIP area.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "recursiveConditionalTypes" --verbose`
  - `FINAL RESULTS: 2/2 passed (100.0%)`
  - `Skipped: 0`
  - `Known failures: 0`
  - `Fingerprint-only: 0`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "mixinAccessModifiers" --verbose`
  - `FINAL RESULTS: 0/1 passed (0.0%)`
  - `Known failures: 1`
- Pre-commit attempted with `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target`.
  It passed formatting and clippy, then failed existing
  `tsz_wrapper::tests::test_compile_no_errors` in `tsz-conformance`; a direct
  trivial `tsz --project` run reproduced `TS2552` from
  `scripts/node_modules/typescript/lib/lib.es2021.intl.d.ts`.
