# [WIP] fix(checker): control-flow function-like circular diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-functionlike-circular-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / missing diagnostics

## Intent

Random conformance pick selected
`TypeScript/tests/cases/compiler/controlFlowFunctionLikeCircular1.ts`.
The test is currently only-missing: `tsz` emits the existing TDZ/circular
subset but misses several `tsc` diagnostics in the multi-file case.

Baseline on `origin/main` (`a1793fcb3d0`):

- Expected codes: `TS1155`, `TS2345`, `TS2355`, `TS2393`, `TS2411`,
  `TS2448`, `TS2451`, `TS2454`, `TS2456`, `TS2502`, `TS2554`, `TS2749`
- Actual codes: `TS1155`, `TS2355`, `TS2448`, `TS2451`, `TS2454`,
  `TS2502`, `TS2749`
- Missing codes: `TS2345`, `TS2393`, `TS2411`, `TS2456`, `TS2554`

Key missing fingerprints:

- `TS2345` on the two `unionOfDifferentReturnType1(true)` calls
- `TS2393` duplicate function implementation diagnostics for repeated
  `function test(...)` declarations across the virtual files
- `TS2411` for property `x` conflicting with a string index signature
- `TS2456` for `type First = typeof arg`
- `TS2554` for calling the predicate-like value with zero arguments

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline captured with
  `./scripts/conformance/conformance.sh run --filter "controlFlowFunctionLikeCircular1" --verbose`
- Planned: focused checker tests for whichever missing diagnostic path is fixed
- Planned: targeted conformance rerun for `controlFlowFunctionLikeCircular1`
