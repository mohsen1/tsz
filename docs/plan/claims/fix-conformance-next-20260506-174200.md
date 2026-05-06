# fix(checker): align co/contra inference inheritance fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-174200`
- **PR**: #4189
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/coAndContraVariantInferences4.ts`.

`tsc` and tsz agree on diagnostic codes `TS2344` and `TS2430`, but the
diagnostic fingerprints differ. This slice will diagnose whether the drift is
type display, diagnostic anchoring, or inheritance/generic inference behavior,
then align the fingerprints through the owning checker/solver boundary without
adding a test-specific suppression.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/constraint_validation.rs`
- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/error_reporter/generics.rs`
- `crates/tsz-checker/tests/large_union_index_merge_regression_tests.rs`
- `crates/tsz-cli/tests/tsc_compat_tests.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker -E 'test(tag_name_indexed_access_base_constraint_satisfies_element_constraints)'`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-cli -E 'test(dom_deprecated_tag_name_map_keeps_element_constraint_under_node_merge)'`
- `./scripts/conformance/conformance.sh run --filter "coAndContraVariantInferences4" --verbose`
