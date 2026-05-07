# [WIP] fix(checker): align call-signature subtype diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-061223`
- **PR**: #4336
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only conformance failure for
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithCallSignatures3.ts`.
`tsc` and `tsz` already agree on the diagnostic codes (`TS2352`, `TS2564`),
but differ in one diagnostic fingerprint. `tsz` pruned the TS2352 emitted
inside the inline generic callback body while resolving the surrounding
overloaded call through the catch-all overload.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/diagnostics.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`
- `scripts/conformance/conformance-baseline.txt`
- `scripts/conformance/conformance-detail.json`
- `scripts/conformance/conformance-snapshot.json`

## Verification

- `cargo nextest run -p tsz-checker ts2352_in_overloaded_callback_body_survives_catch_all_resolution`
- `./scripts/conformance/conformance.sh run --filter "subtypingWithCallSignatures3" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `./scripts/conformance/conformance.sh snapshot` (12582 tests, 12444 passed; target removed from failures; no extra TS2352 failures)
- Pre-commit hook (16074 tests passed across affected crates)
