# fix(parser): align type guard function error fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/parser-type-guard-function-errors-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/2896
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardFunctionErrors.ts`.
The picker reports matching diagnostic codes (`TS1005`, `TS1128`, `TS1131`,
`TS1144`, `TS1434`), so this PR will root-cause the remaining parser
diagnostic message, span, count, or ordering mismatch.

## Resolution

Aligned parser recovery for invalid identifier type-predicate tails outside
return-type positions:

- parameter-list recovery now reports the second missing comma at the predicate
  type name in `a: b is A`;
- index-signature recovery now skips the invalid `is C` tail and defers the
  enclosing type-member close brace so TSC's TS1128 fingerprint appears.

## Files Touched

- `docs/plan/claims/fix-parser-type-guard-function-errors-fingerprint.md`
- `crates/tsz-parser/src/parser/state_declarations.rs`
- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`

## Verification

- `cargo test -p tsz-parser type_predicate_tail`
- `./scripts/conformance/conformance.sh run --filter "typeGuardFunctionErrors" --verbose` (1/1)
- `./scripts/conformance/conformance.sh run --max 200` (200/200)
- `scripts/githooks/pre-commit`
