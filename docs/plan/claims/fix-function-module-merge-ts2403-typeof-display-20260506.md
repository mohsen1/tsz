---
name: Function/module merge TS2403 typeof display
status: claim
timestamp: 2026-05-06 12:56:00
branch: fix/conformance-next-20260506-125527
---

# Claim

Workstream 1 (Diagnostic Conformance and Fingerprints) for
`TypeScript/tests/cases/conformance/internalModules/DeclarationMerging/FunctionAndModuleWithSameNameAndCommonRoot.ts`.

## Scope

Align the TS2403 fingerprint for assigning merged function/namespace values to
function-typed `var` declarations. tsc displays the source type as `typeof Point`,
while tsz currently expands the callable namespace object as
`{ (): { x: number; y: number; }; Origin: { x: number; y: number; }; }`.

## Verification Plan

- Focused checker regression for merged function/namespace TS2403 display.
- `cargo nextest run` for the affected checker regression target.
- `./scripts/conformance/conformance.sh run --filter "FunctionAndModuleWithSameNameAndCommonRoot" --verbose`.
