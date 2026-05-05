# [WIP] fix(checker): recover node CJS emit diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/node-modules-cjs-emit-diagnostics`
- **PR**: #3024
- **Status**: claim
- **Workstream**: conformance / node modules diagnostics

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/node/nodeModulesCJSEmit1.ts`.
tsc reports `TS1192`, `TS2304`, and `TS2882`, while tsz currently reports
only `TS2882`. This PR will root-cause why the CJS/node emit scenario drops
the default-export and missing-name diagnostics, then add the owning-crate
regression coverage.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "nodeModulesCJSEmit1" --verbose`
- Planned: focused Rust regression test in the owning crate.
- Planned: targeted conformance rerun for `nodeModulesCJSEmit1`.
