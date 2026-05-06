# fix(checker): suppress module preserve require global diagnostic

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-110532`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/modulePreserve2.ts`.

`tsc` reports no diagnostics for this mixed module-preserve fixture. tsz
currently emits an extra `TS2591` for `require` in module syntax that imports
from a package with conditional `exports`. This slice suppresses only the false
missing-global diagnostic for import-equals module references and resolved
checked-JS `require(...)` calls, while preserving real missing `require`
diagnostics elsewhere.

## Files Touched

- `crates/tsz-checker/src/error_reporter/name_resolution.rs`
- `crates/tsz-checker/src/types/computation/identifier/resolution.rs`
- `crates/tsz-cli/src/driver/tests.rs`
- `docs/plan/claims/fix-conformance-next-20260506-110532.md`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-110532 CARGO_BUILD_JOBS=2 cargo check -p tsz-checker -p tsz-cli --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-110532 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-cli --lib -E 'test(module_preserve_checked_js_resolved_require_does_not_emit_missing_node_global)'`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-110532 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "modulePreserve2" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases` (`1/1 passed`)
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-110532 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-large-ts-repo-signature-param-reserve-20260506/TypeScript/tests/cases` (`200/200 passed`)

Note: `cargo nextest run -p tsz-checker --test environment_capabilities_tests -E 'test(test_node_global_require_without_explicit_types_emits_ts2591)'` could not run because `environment_capabilities_tests` is not registered as a Cargo test target on this branch.
