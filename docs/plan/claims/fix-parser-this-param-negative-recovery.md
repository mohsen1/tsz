# [WIP] fix(parser): align invalid `this` parameter recovery

- **Date**: 2026-05-05
- **Branch**: `fix/parser-this-param-negative-recovery`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / parser diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/types/thisType/thisTypeInFunctionsNegative.ts`,
a wrong-code failure where tsc reports parser diagnostics
`TS1005`, `TS1109`, `TS1128`, `TS1359`, `TS1433`, and `TS1434`, while tsz
currently reports only `TS1433` plus checker follow-on diagnostics
`TS2353`, `TS2370`, `TS2554`, and `TS2684`.

This PR will root-cause the remaining invalid `this` parameter recovery cases
after PR #1696's modifier-specific fix, add owning parser/checker regression
coverage, and rerun the targeted conformance test.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "thisTypeInFunctionsNegative" --verbose`
- Planned: owning-crate Rust regression test.
- Planned: targeted conformance rerun for `thisTypeInFunctionsNegative`.
