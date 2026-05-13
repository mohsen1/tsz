# Fix conformance aggregate regression cluster

- **Date**: 2026-05-13
- **Branch**: `fix/conformance-aggregate-10-regressions-20260513`
- **PR**: #6494
- **Status**: ready
- **Workstream**: conformance

## Intent

The current PR queue is blocked by a repeated `conformance-aggregate` failure:
six shard jobs pass, but the aggregate reports 12575/12585 against the checked-in
12581 baseline. The reported regressions are the same 10-test cluster across
unrelated PRs, so this branch claims the global root-cause investigation and fix
rather than patching individual PR branches.

## Files Touched

- `crates/conformance/src/runner.rs`
- `crates/conformance/src/tsz_wrapper.rs`
- `crates/tsz-checker/src/context/def_mapping.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/state/type_analysis/cross_file.rs`
- `crates/tsz-checker/src/state/type_analysis/cross_file_direct.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`
- `crates/tsz-core/src/config/mod.rs`
- `crates/tsz-lowering/src/lower/advanced.rs`
- `crates/tsz-lowering/src/lower/core.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`

## Verification

- `cargo test -p tsz-checker --test ts2322_tests test_strict_builtin_iterator_return_in_lib_heritage_displays_undefined -- --nocapture`
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `scripts/conformance/conformance.sh run --filter iterableTReturnTNext --workers 4 --profile dist-fast --test-dir .worktrees/fix-export-equals-require-surface-20260509/TypeScript/tests/cases --verbose`
- `cargo build --profile dist-fast -p tsz-conformance`
- Targeted aggregate regression cluster, all passing 1/1:
  - `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`
  - `iterableTReturnTNext`
  - `typeVariableConstraintIntersections`
  - `recursiveMappedTypes`
  - `callSignatureAssignabilityInInheritance2`
  - `callSignatureAssignabilityInInheritance5`
  - `constructSignatureAssignabilityInInheritance2`
  - `constructSignatureAssignabilityInInheritance5`
  - `recursiveTypeReferences1`
  - `subtypingWithConstructSignatures5`
- `cargo fmt --all --check`
- `git diff --check`
