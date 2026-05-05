# [WIP] fix(checker): preserve TS union display order for constructor guard errors

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next12`
- **PR**: #2947 (follow-up to #2911)
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`typeGuardConstructorClassAndNumber.ts`. The picked failure has matching
diagnostic codes and locations, but tsz prints the union as `C1 | number` where
tsc prints `number | C1` for TS2339 property-access errors in negative
constructor guard branches.

## Files Touched

- `crates/tsz-checker/src/error_reporter/properties.rs`
- `crates/tsz-checker/tests/conformance_issues/types/narrowing.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/typeGuardConstructorClassAndNumber.ts`.
- `cargo fmt --check` passed.
- `git diff --check` passed.
- `cargo nextest run -p tsz-checker --test conformance_issues test_constructor_guard_negative_branch_ts2339_uses_tsc_union_order`
  passed.
- `cargo nextest run -p tsz-checker --test conformance_issues test_union_restricted_indexed_access_prefers_ts2339_over_constraint_failure`
  passed.
- Repo pre-commit hook passed while creating the implementation commit
  (`15019 passed, 54 skipped` in affected-crate nextest).
- `CARGO_BUILD_JOBS=4 ./scripts/conformance/conformance.sh run --filter "typeGuardConstructorClassAndNumber" --verbose`
  passed 1/1.
- Full conformance:
  `CARGO_BUILD_JOBS=4 ./scripts/conformance/conformance.sh run` reported
  `12460/12582 passed (99.0%)`, `Fingerprint-only: 79`, net
  `12451 -> 12460 (+9)`, including
  `typeGuardConstructorClassAndNumber.ts` as an improvement. The reported
  `shebangBeforeReferences.ts` PASS -> FAIL delta was reproduced on clean
  `origin/main`, and `nestedRecursiveArraysOrObjectsError01.ts` was previously
  reproduced on clean `origin/main`, so both are baseline drift rather than this
  property-display-only PR.
