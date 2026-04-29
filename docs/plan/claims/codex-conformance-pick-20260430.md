# [WIP] fix(checker): align mapped type error fingerprints

- **Date**: 2026-04-29
- **Branch**: `codex/conformance-pick-20260430`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

This PR targets the `mappedTypeErrors.ts` conformance failure selected by
`scripts/session/quick-pick.sh`. The diagnostic code set already matches tsc,
so the intended scope is fingerprint parity: message text, count grouping, or
anchor/location behavior for mapped type diagnostics.

## Files Touched

- `docs/plan/claims/codex-conformance-pick-20260430.md` (claim/status)
- Implementation files TBD after verbose conformance diagnosis

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "mappedTypeErrors" --verbose`
- Planned: targeted owning-crate unit tests
