# [WIP] fix(checker): align instanceof Symbol.hasInstance fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instanceof-hasinstance-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardsWithInstanceOfBySymbolHasInstance.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2339`, `TS2551`), so this PR will root-cause the remaining
message, display, count, or anchor mismatch around `instanceof` narrowing with
`Symbol.hasInstance`.

## Files Touched

- `docs/plan/claims/fix-checker-instanceof-hasinstance-fingerprint.md`
  (claim)
- Compiler files TBD after root-cause analysis.
- Owning-crate regression test TBD after root-cause analysis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "typeGuardsWithInstanceOfBySymbolHasInstance" --verbose`
- Owning-crate `cargo nextest run` filter for the new regression test.
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
