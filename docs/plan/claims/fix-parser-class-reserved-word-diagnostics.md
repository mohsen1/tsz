# fix(parser): align class reserved-word diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-class-reserved-word-diagnostics`
- **PR**: #1803
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance mismatch for
`TypeScript/tests/cases/compiler/strictModeReservedWordInClassDeclaration.ts`.
The picked failure missed TS2702 and emitted extra TS1139, TS2300, and TS7051
diagnostics around strict-mode reserved words in class declarations. The fix
preserves future-reserved identifier text during parser recovery, routes class
type parameters and heritage leftmost names through class-strict diagnostics,
and reports TS2702 for type-only left sides in qualified class heritage names.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs`
- `crates/tsz-parser/src/parser/state_expressions.rs`
- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-checker/src/checkers/parameter_checker.rs`
- `crates/tsz-checker/src/state/state_checking/class.rs`
- `crates/tsz-checker/src/state/state_checking/heritage.rs`
- `crates/tsz-checker/src/state/state_checking/heritage_class_recovery.rs`
- `crates/tsz-checker/src/state/state_checking/mod.rs`
- `crates/tsz-checker/src/state/state_checking/strict_names.rs`
- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`
- `crates/tsz-checker/tests/class_reserved_word_diagnostics_tests.rs`

## Verification

- `cargo check --package tsz-checker` (pass)
- `cargo check --package tsz-solver` (pass)
- `cargo build --profile dist-fast --bin tsz` (pass)
- `cargo nextest run --package tsz-parser --lib` (716 tests, 715 passed, 1 skipped)
- `cargo nextest run --package tsz-checker --lib` (3005 tests passed, 10 skipped)
- `cargo nextest run --package tsz-checker --test class_reserved_word_diagnostics_tests` (1 passed)
- `./scripts/conformance/conformance.sh run --filter "strictModeReservedWordInClassDeclaration" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12244/12582 passed (97.3%)`)
