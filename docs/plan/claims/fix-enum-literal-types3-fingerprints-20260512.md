# fix(checker): align enum literal type comparison fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/enum-literal-types3-fingerprints-20260512`
- **Base**: `main`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Investigate and align the remaining fingerprint-only conformance drift in:

- `TypeScript/tests/cases/conformance/types/literal/enumLiteralTypes3.ts`
- `TypeScript/tests/cases/conformance/types/literal/stringEnumLiteralTypes3.ts`

Both tests currently report the same diagnostic code sets as tsc but differ in
one or more diagnostic fingerprints.

## Scope

- Reproduce the focused conformance deltas for both tests.
- Fix the smallest checker/solver diagnostic-boundary root cause needed to match
  tsc fingerprints without weakening enum literal assignability semantics.
- Add focused regression coverage in the owning crate if the root cause is
  isolated.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "enumLiteralTypes3" --verbose`
- `./scripts/conformance/conformance.sh run --filter "stringEnumLiteralTypes3" --verbose`
- Focused Rust regression for the changed checker/solver path
- `cargo fmt --all`
- Pre-commit or equivalent direct-crate validation before marking ready

## Progress

- Claim created.
