# Claim: Fix JSX intrinsic type-argument diagnostics

## Target

`TypeScript/tests/cases/compiler/jsxIntrinsicElementsTypeArgumentErrors.tsx`

Current filtered conformance on `origin/main`: 0/1 passed.

Expected diagnostics include TS1009, TS2304, TS2344, and TS2558 for JSX intrinsic elements with invalid type arguments.

Actual diagnostics currently miss those fingerprints.

## Plan

Investigate JSX opening/self-closing element type-argument parsing, lowering, and validation. Ensure intrinsic JSX elements with type arguments report the same parser/name/constraint/count diagnostics as tsc without changing ordinary JSX elements.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/jsx-intrinsic-type-args CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 RUSTFLAGS='-Cdebuginfo=0' cargo test -p tsz-checker --test jsx_component_attribute_tests test_jsx_intrinsic_type_args_validate_nested_errors -- --nocapture`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/jsx-intrinsic-type-args CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 RUSTFLAGS='-Cdebuginfo=0' cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `/Users/mohsen/code/tsz-build-targets/jsx-intrinsic-type-args/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz-build-targets/jsx-intrinsic-type-args/dist-fast/tsz --filter jsxIntrinsicElementsTypeArgumentErrors --workers 1 --verbose --print-fingerprints --print-test-files --no-batch --timeout 60` - `1/1 passed`
