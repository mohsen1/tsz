# [WIP] fix(checker): align ambient const enum module diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked conformance failure `verbatimModuleSyntaxAmbientConstEnum.ts`.
TSZ currently emits TS2748 but misses TS2300 and TS2432 for the ambient const enum
module scenario. This PR will diagnose the root cause in the parser/binder/checker
boundary and add a focused regression test in the owning crate.

## Files Touched

- TBD after diagnosis

## Verification

- `./scripts/conformance/conformance.sh run --filter "verbatimModuleSyntaxAmbientConstEnum" --verbose`
- Owning crate unit tests via `cargo nextest run`
- Additional targeted checks required by the final touched scope
