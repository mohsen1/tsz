---
name: Strict optional properties remaining fingerprints
status: claim
timestamp: 2026-05-06 14:13:50
branch: fix/conformance-next-20260506-141350
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`.

## Scope

Address a remaining fingerprint-only mismatch in the strict optional property
fixture after the earlier exact-optional display and direct-write fixes.

## Verification Plan

- Focused regression in the owning checker/solver area.
- `cargo nextest run` for the affected crate target.
- `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties1" --verbose`.
