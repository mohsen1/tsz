# [WIP] fix(checker): suppress higher-order retained generics diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-higher-order-retained-generics-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (TypeScript conformance)

## Intent

This PR investigates and fixes the false-positive diagnostics in `declarationEmitHigherOrderRetainedGenerics.ts`, where tsz currently reports TS2345, TS2769, and TS7031 while upstream `tsc` accepts the file. The goal is to align call inference and contextual typing for this higher-order retained generics pattern without suppressing unrelated call diagnostics.

## Files Touched

- TBD after root cause isolation.

## Verification

- TBD.
