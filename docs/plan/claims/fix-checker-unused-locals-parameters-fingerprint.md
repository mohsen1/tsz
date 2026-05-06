# fix(checker): align unused locals/parameters fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-unused-locals-parameters-fingerprint`
- **PR**: #3477
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/unusedLocalsAndParameters.ts`.
Both tsc and tsz emit `TS1005` and `TS1109`, but diagnostic fingerprints do
not match. The planned scope is to identify the exact parser/checker diagnostic
location or message drift and fix it through the existing diagnostic paths.

## Files Touched

- `docs/plan/claims/fix-checker-unused-locals-parameters-fingerprint.md`
- `crates/tsz-parser/src/parser/state_declarations_exports.rs`
- `crates/tsz-parser/tests/state_declaration_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo nextest run -j 1 -p tsz-parser parse_for_typed_let_header_recovers_through_block_like_tsc parse_for_with_var_decl_init_unterminated_emits_comma_expected_at_close_paren --no-tests=fail`
- `./scripts/conformance/conformance.sh run --filter "unusedLocalsAndParameters" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
