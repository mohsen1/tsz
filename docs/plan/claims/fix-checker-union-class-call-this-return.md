# fix(checker): preserve inherited this return identity in union calls

- **Date**: 2026-04-28
- **Branch**: `fix/checker-union-class-call-this-return`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Fix `unionOfClassCalls.ts`, where calling an inherited `Promise<this>` method
through a `Bar | Baz` receiver produces a false TS2345 in the chained `then`
callback. The solver should keep the resolved class instance object assignable
to the nominal class reference that denotes the same non-generic class DefId.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/core.rs`
- `crates/tsz-checker/tests/call_resolution_regression_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker union_receiver_inherited_promise_this_return_preserves_class_identity`
- `cargo nextest run -p tsz-checker --test call_resolution_regression_tests` (134/134 passed)
- `cargo check -p tsz-solver -p tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `.target/dist-fast/tsz-conformance --filter unionOfClassCalls --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 1 --no-batch` (1/1 passed)
- `.target/dist-fast/tsz-conformance --max 200 --verbose --print-fingerprints --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --workers 1 --no-batch` (199/200 passed; pre-existing `aliasOnMergedModuleInterface.ts` TS2708 mismatch)
- `rustfmt --edition 2024 --check crates/tsz-solver/src/relations/subtype/core.rs crates/tsz-checker/tests/call_resolution_regression_tests.rs`
- `git diff --check -- crates/tsz-solver/src/relations/subtype/core.rs crates/tsz-checker/tests/call_resolution_regression_tests.rs docs/plan/claims/fix-checker-union-class-call-this-return.md`
