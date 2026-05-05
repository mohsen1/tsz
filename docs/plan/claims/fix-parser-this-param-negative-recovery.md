# [WIP] fix(parser): align invalid `this` parameter recovery

- **Date**: 2026-05-05
- **Branch**: `fix/parser-this-param-negative-recovery`
- **PR**: #3206
- **Status**: ready
- **Workstream**: 1 (Conformance / parser diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/types/thisType/thisTypeInFunctionsNegative.ts`,
a wrong-code failure where tsc reports parser diagnostics
`TS1005`, `TS1109`, `TS1128`, `TS1359`, `TS1433`, and `TS1434`, while tsz
currently reports only `TS1433` plus checker follow-on diagnostics
`TS2353`, `TS2370`, `TS2554`, and `TS2684`.

This PR root-causes the remaining invalid `this` parameter recovery cases
after PR #1696's modifier-specific fix, adds owning parser regression
coverage, and reruns the targeted conformance test.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/this_param_modifier_tests.rs`
- `docs/plan/claims/fix-parser-this-param-negative-recovery.md`

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "thisTypeInFunctionsNegative" --verbose`
- `CARGO_BUILD_JOBS=1 cargo nextest run -p tsz-parser --lib this_param`
  - 13 tests passed.
- `CARGO_BUILD_JOBS=1 ./scripts/conformance/conformance.sh run --filter "thisTypeInFunctionsNegative" --verbose`
  - Final results: 1/1 passed (100.0%).
