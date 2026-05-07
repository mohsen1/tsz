# fix(checker): reduce recursive type references extra diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/recursive-type-references1-extra-diagnostics-20260507-000000`
- **PR**: https://github.com/mohsen1/tsz/pull/4334
- **Status**: draft PR open
- **Workstream**: 1 (Conformance fixes)

## Intent

Target
`TypeScript/tests/cases/conformance/types/typeRelationships/recursiveTypes/recursiveTypeReferences1.ts`.
The canonical picker reports extra diagnostics: expected `TS2304,TS2322`,
actual `TS2304,TS2322,TS2339,TS7006,TS7031`. This slice will identify why
recursive type-reference recovery leaks extra property and implicit-any
diagnostics, then fix the owning checker path without suppressing the expected
missing-name and assignability diagnostics.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/global.rs`
- `crates/tsz-checker/tests/lib_resolution_identity_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test lib_resolution_identity_tests test_recursive_alias_interface_preserves_array_method_surface`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker --test lib_resolution_identity_tests -E 'test(test_recursive_alias_interface_preserves_array_method_surface) or test(test_merge_global_augmentations_with_declare_global) or test(test_lib_global_augmentation_merges_with_stable_def_id)'`
- Pre-commit hook passed: clippy, wasm rustc warnings gate, checker
  boundary guardrail, and 16065 nextest tests.
- `./scripts/conformance/conformance.sh run --filter "recursiveTypeReferences1" --verbose`
  now matches the expected diagnostic code set (`TS2304,TS2322`) and removes
  the extra `TS2339,TS7006,TS7031` diagnostics. The run still fails as
  fingerprint-only due to pre-existing TS2322 recursive array inference
  location/message differences.
