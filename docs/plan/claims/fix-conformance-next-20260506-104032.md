# fix(parser): align rest parameter modifier diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-104032`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/restParamModifier.ts`.

`tsc` reports `TS1005` for the invalid constructor rest parameter modifier.
tsz currently also emits an extra `TS1213` strict-mode reserved-word/modifier
diagnostic on the recovered rest parameter. This slice will find the parser or
checker recovery path that emits the follow-up and suppress only the duplicate
diagnostic after the syntax error is already reported.

## Files Touched

- `crates/tsz-checker/src/checkers/parameter_checker.rs`
- `docs/plan/claims/fix-conformance-next-20260506-104032.md`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-104032 CARGO_BUILD_JOBS=2 cargo check -p tsz-checker --lib`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-104032 CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --lib -E 'test(recovered_rest_parameter_modifier_suppresses_class_strict_reserved_name)'`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-104032 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "restParamModifier" --verbose --test-dir /Users/mohsen/code/tsz/.worktrees/conformance-next-20260506-090131/TypeScript/tests/cases` (`2/2 passed`)
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz-build-targets/conformance-next-20260506-104032 CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --max 200 --test-dir /Users/mohsen/code/tsz/.worktrees/conformance-next-20260506-090131/TypeScript/tests/cases` (`200/200 passed`)

## Notes

- A broader `cargo nextest run -p tsz-checker -E 'test(recovered_rest_parameter_modifier_suppresses_class_strict_reserved_name)'`
  compile was blocked by the unrelated existing duplicate
  `test_zod_like_recursive_class_constraints_do_not_emit_ts2313` definition in
  `crates/tsz-checker/tests/cross_module_nested_interface_tests.rs` on this
  base. The focused library-target nextest command above passed.
