# fix(checker): unsuppress intersectionsOfLargeUnions2 select index

- **Date**: 2026-05-12 06:31:38 UTC
- **Branch**: `fix/prune-intersections-xfail-mainbase-20260512`
- **PR**: #5755
- **Status**: implemented
- **Workstream**: 1 (Conformance - diagnostic pass-rate fix)

## Intent

The focused conformance runner still carried production suppression debt for
`TypeScript/tests/cases/compiler/intersectionsOfLargeUnions2.ts`.

After the current large-union fixes, the only remaining mismatch was a false
`TS2430` from default-lib revalidation:
`HTMLSelectElement` was reported as incorrectly extending `HTMLElement` after a
user-side numeric index merge into `HTMLElement`. The numeric index value on
`HTMLSelectElement` is a union of option/group element types whose declarations
inherit from `HTMLElement`, but the evaluated lib object shapes can lose the
lazy definition surface used by the raw assignability path.

## Files Touched

- `crates/tsz-checker/src/classes/class_checker_compat.rs`
- `crates/tsz-checker/src/classes/interface_heritage_index_compat.rs`
- `crates/tsz-checker/src/classes/mod.rs`
- `crates/conformance/src/runner.rs`
- `crates/tsz-cli/src/driver/check.rs`

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `TSZ_LIB_DIR=/Users/mohsen/code/tsz/TypeScript/lib CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-cli --lib default_lib_validation_keeps_select_option_index_compatible_after_html_element_merge`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test large_union_index_merge_regression_tests`
- `TSZ_LIB_DIR=/Users/mohsen/code/tsz/TypeScript/lib CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target /Users/mohsen/code/tsz/.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz-xfail-intersections-20260512/scripts/conformance/tsc-cache-full.json --tsz-binary /Users/mohsen/code/tsz/.target/dist-fast/tsz --filter intersectionsOfLargeUnions2 --verbose --print-fingerprints --workers 1`
