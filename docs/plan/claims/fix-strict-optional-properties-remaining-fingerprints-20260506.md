---
name: Strict optional properties exact optional presence fingerprints
status: ready
timestamp: 2026-05-06 14:13:50
branch: fix/conformance-next-20260506-141350
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`.

## Scope

Address the exact-optional property-presence portion of the remaining
fingerprint-only mismatch in the strict optional property fixture after the
earlier exact-optional display and direct-write fixes.

This work removes the stale TS2412/TS2322 fingerprints around
`obj.a = obj.a` under `'a' in obj` and `obj.hasOwnProperty('a')`. The fixture
still has independent tuple-hole and index-signature fingerprints to address in
follow-up workstreams.

## Verification Plan

- Focused regression in the owning checker/solver area.
- `cargo nextest run` for the affected crate target.
- `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties1" --verbose`
  remains fingerprint-only, with the exact-optional presence fingerprints cleared.
