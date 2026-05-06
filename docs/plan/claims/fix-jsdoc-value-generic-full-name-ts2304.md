# fix(checker): suppress full-name TS2304 for JSDoc function-value generic bases

- **Date**: 2026-05-06
- **Branch**: `fix/jsdoc-value-generic-full-name-ts2304`
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Match `tsc` for JSDoc generic-looking references whose base is a known
function value. For `@param {fn<T>}`, TypeScript reports the unresolved type
argument `T` but does not also report `Cannot find name 'fn<T>'` when `fn` is
declared as a function value. Keep the existing TS2315 behavior for
non-generic type bases such as `Boolean<T>` and `Void<Missing>`.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics.rs`
- `crates/tsz-checker/tests/jsdoc_type_expression_tests.rs`

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `./scripts/arch/check-checker-boundaries.sh`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo build -p tsz-cli -p tsz-conformance --bin tsz --bin tsz-conformance`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/next-main-scan CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker jsdoc_nongeneric_instantiation_reports_ts2315_and_ts2304 -- --nocapture`
  - `1 matching test passed`
- Targeted conformance before this slice on current `main`:
  - `jsdocTypeNongenericInstantiationAttempt`: `0/1 passed`
  - Extra fingerprint: `TS2304 index8.js:4:12 Cannot find name 'fn<T>'.`
- Targeted conformance after this slice:
  - `jsdocTypeNongenericInstantiationAttempt`: `1/1 passed`
  - `jsdocType`: `25/25 passed`
- Net targeted delta: `+1` passing conformance test
- Pre-commit hook:
  - `cargo fmt`, affected-crate clippy, wasm rustc warnings, and checker
    boundary guardrail passed.
  - The hook's final repo-local test link failed with `No space left on device`
    after generating `.target`; focused tests and targeted conformance above
    used the external target directory.

## Notes

- A full conformance run was attempted with `--workers 8`, but the host temp
  directory ran out of space mid-run and started producing `No space left on
  device` errors. That aggregate is invalid and not counted for this claim.
