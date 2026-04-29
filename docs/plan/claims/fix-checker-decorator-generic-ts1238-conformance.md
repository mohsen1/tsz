# [WIP] fix(checker): emit TS1238 for generic class decorator constraints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-decorator-generic-ts1238-conformance`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the picked conformance failure `decoratorCallGeneric.ts`, where `tsc` reports TS1238 for a generic class decorator constraint mismatch but `tsz` currently emits no diagnostic. This follows the existing roadmap investigation for `fix/checker-ts1238-generic-decorator-call` and will root-cause whether the gap is in lib-loaded class constructor shape comparison, call inference, or decorator-call validation.

## Files Touched

- `docs/plan/claims/fix-checker-decorator-generic-ts1238-conformance.md` (claim/status)
- Implementation files TBD after diagnosis

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "decoratorCallGeneric" --verbose`
- Planned: owning crate unit tests via `cargo nextest run`
