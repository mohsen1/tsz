# fix(parser): align static-block await target diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-static-block-await-target-fingerprint`
- **PR**: #1831
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Align the remaining `classStaticBlock26.ts` diagnostic fingerprints around
bare `await` inside class static blocks. The root cause was parser recovery:
TSZ treated static-block `await` as an arrow parameter candidate and reported
bare computed `[await]` at the keyword instead of the closing bracket.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs`
- `crates/tsz-parser/src/parser/state_expressions.rs`
- `crates/tsz-parser/src/parser/state_expressions_literals.rs`
- `crates/tsz-parser/src/parser/state_statements.rs`
- `crates/tsz-parser/tests/state_statement_tests.rs`

## Verification

- `cargo check -p tsz-parser`
- `cargo nextest run -p tsz-parser` — 732 passed, 1 skipped
- `cargo nextest run -p tsz-parser -- static_block` — 7 passed
- `./scripts/conformance/conformance.sh run --filter "classStaticBlock26" --verbose` — 1/1 passed
- `./scripts/conformance/conformance.sh run --max 200` — 200/200 passed
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` — 12271/12582 passed
