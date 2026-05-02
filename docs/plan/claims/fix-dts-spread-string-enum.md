# [WIP] fix(emitter): preserve spread enum key order

- **Date**: 2026-05-02
- **Branch**: `fix/dts-spread-string-enum`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Fix `declarationEmitSpreadStringlyKeyedEnum`, where declaration emit prints
the correct spread enum object members but in a non-tsc order. The target is a
narrow ordering fix that preserves enum declaration order for stringly keyed
enum spread object types.

## Files Touched

- TBD after focused implementation.

## Verification

- Focused emit repro for `declarationEmitSpreadStringlyKeyedEnum`.
