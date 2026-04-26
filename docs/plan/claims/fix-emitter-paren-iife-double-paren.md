# fix(emitter): drop double parens around IIFE / type-cast object literal at statement position

- **Date**: 2026-04-26
- **Branch**: `fix/emitter-paren-iife-double-paren`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS emit pass-rate)

## Intent

The JS emitter produced double parens for two related shapes:

1. `(<any>function foo() { })()` → `((function foo() { }))()` (expected
   `(function foo() { })();`).
2. `(<any>{ a: 0 });` at statement position → `(({ a: 0 }));` (expected
   `({ a: 0 });`).

Root cause: the surviving source `ParenthesizedExpression` already provides
leading-token disambiguation, but the printer still applied either
`paren_leftmost_function_or_object` (for the IIFE callee) or the
expression-statement wrap on top of it. This PR clears the
self-parenthesize flag inside `emit_parenthesized` and adds a
`outer_paren_will_survive_emit` short-circuit in
`emit_expression_statement` so the source paren is reused exactly once.

Fixes the `castExpressionParentheses` and `noImplicitAnyInCastExpression`
emit baselines (JS pass count: 12327 → 12329 net delta on the local
worktree). No declaration emit movement; no JS regressions.

## Files Touched

- `crates/tsz-emitter/src/emitter/expressions/core/helpers.rs` (~10 LOC)
  — clear `paren_leftmost_function_or_object` inside the surviving-paren
  emit branch.
- `crates/tsz-emitter/src/emitter/statements/core.rs` (~75 LOC)
  — add `outer_paren_will_survive_emit` and short-circuit
  `needs_parens` when the source paren survives.
- `crates/tsz-emitter/Cargo.toml` (+4 LOC) — register new test target.
- `crates/tsz-emitter/tests/parenthesized_iife_tests.rs` (new, ~115 LOC)
  — six regression tests using the `test_support` fixture from PR #1238.

Net LOC: about +130 lines (mostly comments + tests).

## Verification

- `cargo nextest run -p tsz-emitter` (1616 tests pass, 2 skipped — six
  new tests in `parenthesized_iife_tests`).
- `cargo nextest run -p tsz-checker --lib` (2821 tests pass, 9 skipped —
  no behavioral coupling).
- `cargo clippy -p tsz-emitter --tests --no-deps -- -D warnings`: clean.
- `python3 scripts/arch/arch_guard.py`: passes.
- `scripts/emit/run.sh --js-only`: 12327 → 12329 (+2 tests:
  `castExpressionParentheses`, `noImplicitAnyInCastExpression`); 0
  regressions.
- `scripts/emit/run.sh --dts-only`: 1284 (no change).
