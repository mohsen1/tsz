# fix(checker): emit TS7008 for null JS constructor provisional members

- **Date**: 2026-05-12
- **Branch**: `fix/js-null-constructor-ts7008-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Restore `typeFromJSInitializer` conformance by emitting TS7008 for checked-JS constructor members initialized with `null` under `noImplicitAny`, while preserving the open `any` write surface for later assignments.

## Verification

Pending.
