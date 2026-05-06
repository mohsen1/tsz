# fix(checker): match external global types-field diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-external-global-types-field-diagnostics`
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Match `tsc` for missing external global identifiers such as Node globals,
jQuery globals, and test-runner globals. In the name-resolution path, current
`tsc` emits the TS2591/TS2592/TS2593 diagnostics that include the "add to the
types field in your tsconfig" instruction. The install-only TS2580 path remains
available for module-resolution diagnostics, where the unresolved text is a
module specifier rather than a global identifier.

## Files Touched

- `crates/tsz-checker/src/error_reporter/name_resolution.rs`
- `crates/tsz-checker/tests/environment_capabilities_tests.rs`
- `crates/tsz-cli/tests/driver_tests.rs`
- `crates/tsz-common/src/options/checker.rs`

## Verification

- `cargo fmt --all --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-cli missing_external_globals -- --nocapture`
  - `2 tests passed`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-cli checked_js_node_globals_match_tsc_scope -- --nocapture`
  - `1 test passed`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker test_node_global_require_without_explicit_types_emits_ts2591 -- --nocapture`
  - `1 matching test passed`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo build -p tsz-cli -p tsz-conformance --bin tsz --bin tsz-conformance`
- `git diff --check`
- `./scripts/arch/check-checker-boundaries.sh`
- Targeted conformance filters:
  - `didYouMeanSuggestionErrors`: `1/1 passed`
  - `moduleExports1`: `1/1 passed`
  - `typecheckIfCondition`: `1/1 passed`
  - `parser509693`: `1/1 passed`
  - `typingsSuggestion2`: `1/1 passed`
  - `metadataImportType`: `1/1 passed`
  - `Quote'InName`: `2/2 passed`
  - `reference-1`: `10/10 passed`
  - `matchFiles`: `1/1 passed`
  - `importTypeWithUnparenthesizedGenericFunctionParsed`: `1/1 passed`
  - `importAliasInModuleAugmentation`: `1/1 passed`
  - `rewriteRelativeImportExtensions`: `6/6 passed`
  - `undeclaredModuleError`: `1/1 passed`
  - `typingsSuggestion1`: `1/1 passed`
  - `VisibilityOfCrosssModuleTypeUsage`: `3/3 passed`
  - `declarationEmitTripleSlashReferenceAmbientModule`: `1/1 passed`
  - `elidedJSImport1`: `1/1 passed`
- Full conformance:
  - `/Users/mohsen/code/tsz-build-targets/next-main-scan/debug/tsz-conformance --test-dir /Users/mohsen/code/tsz-worktrees/origin-main-20260505-7/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-build-targets/next-main-scan/debug/tsz --workers 8 --print-test --print-fingerprints`
  - `FINAL RESULTS: 12459/12582 passed (99.0%)`
  - Current-main baseline before this slice: `12450/12582 passed (99.0%)`
  - Net delta: `+9` passing tests
- Pre-commit hook:
  - `cargo fmt`, clippy, wasm rustc warnings, and checker boundary guardrail passed.
  - The hook's final repo-local test build failed with `No space left on device` after generating `.target`; focused tests and full conformance above used the external target directory.
