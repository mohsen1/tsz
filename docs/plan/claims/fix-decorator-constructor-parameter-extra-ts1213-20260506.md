---
name: Decorator constructor parameter extra TS1213
status: claim
timestamp: 2026-05-06 13:39:32
branch: fix/conformance-next-20260506-133932
---

# Claim

Workstream 1 (Diagnostic Conformance) for
`TypeScript/tests/cases/conformance/decorators/class/constructor/parameter/decoratorOnClassConstructorParameter4.ts`.

## Scope

Remove the extra TS1213 emitted while recovering from an invalid decorator
position in a constructor parameter property.

## Verification Plan

- Focused parser regression for `constructor(public @dec p: number)`.
- `cargo nextest run` for the affected parser regression target.
- `./scripts/conformance/conformance.sh run --filter "decoratorOnClassConstructorParameter4" --verbose`.
