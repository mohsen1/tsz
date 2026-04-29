# [WIP] fix(parser): align labeled loop declaration diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-labeled-loop-decl-fingerprint`
- **PR**: #1713
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

- `crates/tsz-parser/src/parser/state_expressions_literals.rs`
- `crates/tsz-parser/tests/state_statement_tests.rs`
- `docs/plan/claims/fix-parser-labeled-loop-decl-fingerprint.md`

## Verification

- `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS='-C debuginfo=0' cargo nextest run --package tsz-parser parse_unterminated_template_recovery_reports --no-fail-fast` (2 tests pass)
- Blocked by local disk space: `CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUSTFLAGS='-C debuginfo=0' ./scripts/conformance/conformance.sh run --filter "labeledStatementDeclarationListInLoopNoCrash4" --verbose` failed during dist-fast dependency build with `No space left on device`.
