# [WIP] fix(checker): report conflicting recursive interface bases

- **Date**: 2026-05-03
- **Branch**: `fix/ts2320-complex-recursive-collections-05031424`
- **PR**: #2571
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `complexRecursiveCollections.ts`, where tsc reports
TS2320 for interfaces that simultaneously extend recursive collection bases
while tsz reports lower-level TS2430 inheritance diagnostics instead. The fix
should preserve the underlying assignability checks but choose the tsc-parity
diagnostic when multiple inherited base interfaces conflict at the same
declaration.

## Files Touched

- `crates/tsz-checker/src/classes/class_checker_compat.rs`
- `crates/tsz-checker/src/classes/class_checker_compat_overloads.rs`
- `crates/tsz-checker/tests/ts2320_tests.rs`
- `crates/tsz-checker/tests/ts2430_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` (selected and reproduced
  `complexRecursiveCollections.ts`; missing TS2320 with extra TS2430
  fingerprints).
- `cargo check -p tsz-checker`
- `cargo nextest run -p tsz-checker ts2320`
- `cargo nextest run -p tsz-checker ts2430`
- `cargo nextest run -p tsz-checker architecture_contract_tests_src::test_solver_imports_go_through_query_boundaries architecture_contract_tests_src::test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning`
- `cargo build --profile dist-fast --bin tsz >/tmp/tsz-build.log 2>&1 && ./scripts/conformance/conformance.sh run --filter "complexRecursiveCollections" --verbose`
- `scripts/safe-run.sh cargo nextest run -p tsz-checker`
