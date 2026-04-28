# fix(parser): align top-level await recovery fingerprint

- **Date**: 2026-04-27
- **Branch**: `fix-parser-top-level-await-fingerprint`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Align parser diagnostic fingerprints for `topLevelAwaitErrors.1.ts`, where TSZ already emits the expected TS1005/TS1109 codes but differs from TypeScript in recovery location/message details. The intended scope is the parser's `await` expression recovery around invalid type-argument-like syntax and decorator expressions.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/state_expression_tests.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "topLevelAwaitErrors.1" --verbose` (4/4 passed)
- `cargo test -p tsz-parser await_in_` (2 tests pass)
- `cargo check -p tsz-parser`
