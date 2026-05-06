# fix(checker): align keyof indexed access error fingerprints

- **Status**: claim
- **Branch**: `fix/conformance-next-20260506-5`
- **Target**: `TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccessErrors.ts`
- **Picked**: 2026-05-06 via `scripts/session/quick-pick.sh --show-source`

## Scope

Align the fingerprint-only diagnostics for `keyofAndIndexedAccessErrors.ts`, starting with index-type diagnostic spans/messages and then the generic indexed-assignment TS2322 displays if needed.

## Current mismatch

Exact conformance reports matching code families but drift in TS2537, TS2538, and TS2322 fingerprints.

## Verification

- `./scripts/conformance/conformance.sh run --filter "keyofAndIndexedAccessErrors" --verbose`
