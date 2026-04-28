# [WIP] fix(checker): align narrowed-never TS2322 assignment display

- **Date**: 2026-04-28
- **Branch**: `fix/checker-narrowing-union-never-assignment-display`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only TS2322 mismatch in
`TypeScript/tests/cases/compiler/narrowingUnionToNeverAssigment.ts`, selected by
`scripts/session/quick-pick.sh`. The initial scope is the assignment diagnostic
display path for union narrowing to `never`; implementation will follow the
shared checker/solver boundary rules in `.claude/CLAUDE.md`.

## Files Touched

- `docs/plan/claims/fix-checker-narrowing-union-never-assignment-display.md`
- Implementation files TBD after targeted diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "narrowingUnionToNeverAssigment" --verbose`
- Planned: owner-crate unit tests with `cargo nextest run`
