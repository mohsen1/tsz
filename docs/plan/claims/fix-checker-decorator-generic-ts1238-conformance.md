# [WIP] fix(checker): emit TS1238 for generic class decorator constraints

- **Date**: 2026-04-29
- **Branch**: `fix/checker-decorator-generic-ts1238-conformance`
- **PR**: #1712
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the picked conformance failure `decoratorCallGeneric.ts`, where `tsc` reports TS1238 for a generic class decorator constraint mismatch but `tsz` currently emits no diagnostic. This follows the existing roadmap investigation for `fix/checker-ts1238-generic-decorator-call` and will root-cause whether the gap is in lib-loaded class constructor shape comparison, call inference, or decorator-call validation.

## Files Touched

- `docs/plan/claims/fix-checker-decorator-generic-ts1238-conformance.md` (claim/status)
- Implementation files TBD after diagnosis

## Verification

- `./scripts/conformance/conformance.sh run --filter "decoratorCallGeneric" --verbose` reproduced the picked failure: expected TS1238, actual no diagnostics.
- `cargo nextest run -p tsz-checker --test ts1238_generic_decorator_tests -- ts1238_generic_decorator_constraint_mismatch_emits_with_target_es2015` passes in the no-lib checker utility path.
- A local lib-loaded checker regression test reproduces the conformance miss, but no sound implementation fix is ready yet; speculative solver inference and checker fallback edits were reverted.
