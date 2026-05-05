# [WIP] fix(checker): align styled-components instantiation-limit fingerprint

- **Date**: 2026-05-06
- **Branch**: `codex/conformance-next-20260506-233717`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only `TS2344` mismatch in
`TypeScript/tests/cases/compiler/styledComponentsInstantiaionLimitNotReached.ts`.
The checked-in `origin/main` conformance snapshot produced three earlier random
picker targets that are already claimed and merged, so this claim uses the
current refreshed failure list from PR #3454 while keeping the implementation
branch based on `origin/main`.

## Files Touched

- TBD after investigation.

## Verification

- Planned: focused checker unit test in the owning crate.
- Planned: `./scripts/conformance/conformance.sh run --filter "styledComponentsInstantiaionLimitNotReached" --verbose`
