# fix(checker): align weak symbol diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-weak-symbol-fingerprints-v2`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/dissallowSymbolAsWeakType.ts`.

Current `origin/main` reports the expected TS2345/TS2769 codes, but the
diagnostic fingerprints differ. The WeakSet and WeakMap constructor overload
failures are anchored on the nested array literal instead of the constructor
call, and the direct WeakSet/WeakMap/WeakRef/FinalizationRegistry method calls
do not surface the expected TS2345 fingerprints.

## Verification

- Pending.
