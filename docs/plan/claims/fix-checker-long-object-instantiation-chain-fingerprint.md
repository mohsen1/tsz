# [WIP] fix(checker): align long object instantiation chain fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-long-object-instantiation-chain-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/3366
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/longObjectInstantiationChain3.ts`, where
`tsc` and `tsz` both emit `TS2339` but the diagnostic fingerprint differs for
property access through a long generic object merge chain.

## Files Touched

- `docs/plan/claims/fix-checker-long-object-instantiation-chain-fingerprint.md`

## Verification

- `./scripts/conformance/conformance.sh run --filter "longObjectInstantiationChain3" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
