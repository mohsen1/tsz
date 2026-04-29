# fix(parser): align private identifier optional-chain TS18030 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-private-identifier-chain-ts18030-fingerprint`
- **PR**: #1744
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fixes the fingerprint-only TS18030 mismatch for
`TypeScript/tests/cases/conformance/expressions/optionalChaining/privateIdentifierChain/privateIdentifierChain.1.ts`.
The parser already rejected direct `expr?.#x` accesses, but normal `.` access
continuations never checked whether their left side was already an optional
chain. The parser now reports TS18030 for `expr?.a.#x` and `expr?.m().#x` at
the private identifier, matching tsc.

## Files Touched

- `docs/plan/claims/fix-checker-private-identifier-chain-ts18030-fingerprint.md`
- `crates/tsz-parser/src/parser/state_expressions.rs`
- `crates/tsz-parser/tests/parser_unit_tests.rs`

## Verification

- `cargo fmt --check` (pass)
- `cargo check --package tsz-parser` (pass)
- `cargo check --package tsz-checker` (pass)
- `cargo check --package tsz-solver` (pass)
- `cargo build --profile dist-fast --bin tsz` (pass)
- `cargo nextest run -p tsz-parser -E 'test(private_identifier_optional_chain_continuations_report_ts18030)'` (1/1)
- `cargo nextest run --package tsz-parser` (706/706)
- `cargo nextest run --package tsz-checker --lib` (2964/2964)
- `cargo nextest run --package tsz-solver --lib` (5545/5545)
- `./scripts/conformance/conformance.sh run --filter "privateIdentifierChain.1" --verbose` (1/1)
- `./scripts/conformance/conformance.sh run --max 200` (200/200)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12239/12582 passed (97.3%)`)
- pre-commit hook affected-crate checks (20827/20827)
