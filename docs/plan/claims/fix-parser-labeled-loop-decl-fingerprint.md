# [WIP] fix(parser): align labeled loop declaration diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-labeled-loop-decl-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR investigates and fixes the fingerprint-only mismatch for
`TypeScript/tests/cases/conformance/statements/labeledStatements/labeledStatementDeclarationListInLoopNoCrash4.ts`.
The target currently reports the same diagnostic codes as `tsc` (`TS1005`,
`TS1134`, `TS1135`, `TS1160`) but differs in message text and/or diagnostic
anchors. The fix should preserve parser recovery while aligning the diagnostic
surface with `tsc`.

## Files Touched

- `docs/plan/claims/fix-parser-labeled-loop-decl-fingerprint.md` (claim)
- Parser files TBD after verbose diagnosis.

## Verification

- Planned: `cargo nextest run --package tsz-parser --lib`
- Planned: `./scripts/conformance/conformance.sh run --filter "labeledStatementDeclarationListInLoopNoCrash4" --verbose`
- Planned: quick conformance regression check for nearby parser failures.
