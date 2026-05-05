# fix(parser): align fuzz array recovery fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/parser-fuzz-array-recovery-fingerprint`
- **PR**: #3289
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/parser/ecmascript5/Fuzz/parser0_004152.ts`.
TypeScript keeps the numeric comma continuations from the malformed class field
initializer on `TS1005`, but recovers the trailing `NoMove,` as a bare member
name followed by an invalid class member separator. Align tsz with that
fingerprint by reporting `TS1434` at `NoMove` and `TS1068` at the following
comma instead of collapsing that comma into another `TS1005`.

## Files Touched

- `docs/plan/claims/fix-parser-fuzz-array-recovery-fingerprint.md`
- `crates/tsz-parser/src/parser/state_statements_class_members.rs`
- `crates/tsz-parser/tests/state_statement_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-parser class_field_initializer_comma_continuation_prefers_semicolon_error --no-tests=pass`
- `./scripts/conformance/conformance.sh run --filter "parser0_004152" --verbose`
- `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-parser state_statement_tests --no-tests=pass`
- `scripts/githooks/pre-commit`
