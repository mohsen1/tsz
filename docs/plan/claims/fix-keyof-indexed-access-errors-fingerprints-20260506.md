# fix(checker): align keyof indexed access error fingerprints

- **Status**: claim
- **Branch**: `fix/conformance-next-20260506-5`
- **Target**: `TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccessErrors.ts`
- **Picked**: 2026-05-06 via `scripts/session/quick-pick.sh --show-source`

## Scope

Align the index-type diagnostic fingerprints for `keyofAndIndexedAccessErrors.ts` by fixing TS2537/TS2538 spans, duplicate same-span emissions, and invalid union member messages. The remaining generic indexed-assignment TS2322 drift is out of scope for this PR.

## Current mismatch

Initial exact conformance reported matching code families but drift in TS2537, TS2538, and TS2322 fingerprints. After this change, TS2537/TS2538 fingerprints match; the remaining mismatch is limited to TS2322 generic indexed-assignment fingerprints.

## Verification

- `cargo nextest run -p tsz-checker --test conformance_issues indexed_access`
- `./scripts/conformance/conformance.sh run --filter "keyofAndIndexedAccessErrors" --verbose`
