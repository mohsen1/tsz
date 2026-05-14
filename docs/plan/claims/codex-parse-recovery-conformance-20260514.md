# Claim: parse-recovery conformance cleanup

Status: PR
Owner: Codex
Date: 2026-05-14
PR: #6685

## Intent

Reduce remaining conformance mismatches by aligning parser-recovery suppression with TypeScript for cases where syntax recovery diagnostics should suppress downstream checker grammar diagnostics.

## Scope

- Suppress checker-emitted type-predicate target diagnostics when a source file already has parse diagnostics.
- Treat `TS1389` as a real parser recovery diagnostic for program-level suppression.
- Suppress `TS1315` cascades when the program already has real syntax recovery diagnostics.

## Validation

- `typeGuardFunctionErrors` focused conformance: passed.
- `umd-errors` focused conformance: passed.
- Full conformance snapshot using pinned TypeScript corpus: `12585` tests, `12583` passed, `2` failed.

## Residual

The remaining snapshot failures are in `compiler`; `longObjectInstantiationChain3.ts` has matching `TS2339` code sets and needs separate fingerprint/crash-accounting investigation.
