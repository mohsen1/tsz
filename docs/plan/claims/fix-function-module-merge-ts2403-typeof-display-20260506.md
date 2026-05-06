---
name: Function/module merge TS2403 typeof display
status: ready
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

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib -E 'test(fundule_redecl_uses_typeof_value_display_in_message)'`
- `./scripts/conformance/conformance.sh run --filter "FunctionAndModuleWithSameNameAndCommonRoot" --verbose` (1/1 passed; fingerprint-only 0)
