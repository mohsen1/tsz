# fix(parser,emitter): drop synthesized empty shorthand from extra commas in object literal

- **Date**: 2026-04-26
- **Branch**: `fix/parser-emitter-empty-shorthand-from-comma`
- **PR**: (to be created)
- **Status**: claim
- **Workstream**: 2 (JS Emit pass rate)

## Intent

Source `Boolean({ x: 0,, });` (TypeScript test `parseErrorDoubleCommaInCall`)
emitted `Boolean({\n    x: 0,\n    ,,\n});` instead of tsc's
`Boolean({\n    x: 0,\n});`. The cause is a coordination bug between parser
error recovery and emitter:

1. `parse_property_name` on a `,` token consumes the comma and synthesizes a
   fake `Identifier` node that wraps it. `parse_property_assignment` then
   wraps that into a `SHORTHAND_PROPERTY_ASSIGNMENT` whose source text is
   the comma itself.
2. The object-literal emitter prints this synthesized shorthand using the
   source text (and the `find_token_end_before_trivia` heuristic that asks
   "is the previous byte a comma?"), producing the stray `,,` line.

## Fix

Two coordinated, behavior-preserving changes:

- **Parser** (`state_expressions_literals.rs::parse_property_name`): when the
  current token is one of the object-literal terminators/separators
  (`,`, `}`, `;`, EOF), emit the `TS1136 Property assignment expected`
  diagnostic but do NOT consume the token. Return a zero-width empty
  `Identifier` so the outer `parse_object_literal` loop sees the separator
  and recovers cleanly.
- **Emitter** (`expressions/literals.rs::emit_object_literal`): skip
  `SHORTHAND_PROPERTY_ASSIGNMENT` placeholders whose name is a zero-width
  empty `Identifier`. These are unambiguously the parser's synthesized
  recovery placeholders and have no source text to print.

This satisfies CLAUDE.md §13 (the emitter still owns formatting decisions
and never performs semantic validation; it only filters placeholder nodes
the parser flagged as recovery artifacts).

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions_literals.rs` (~20 LOC,
  conditional in `parse_property_name`'s `_` branch).
- `crates/tsz-emitter/src/emitter/expressions/literals.rs` (~10 LOC, guard
  in the multi-line object literal emit loop).
- `crates/tsz-emitter/tests/object_literal_recovery_tests.rs` (new file,
  3 regression tests).
- `crates/tsz-emitter/Cargo.toml` (register the new test target).

## Verification

- `cargo nextest run -p tsz-parser --lib` — 666 tests pass.
- `cargo nextest run -p tsz-emitter --lib` — 1606 tests pass.
- `cargo nextest run -p tsz-emitter --test object_literal_recovery_tests`
  — 3 new tests pass.
- `./scripts/emit/run.sh --filter=parseErrorDoubleCommaInCall --js-only`
  — 1/1 passes (was 0/1 before fix).
- `./scripts/emit/run.sh --filter=objectLiteral --js-only` — 135/162 pass
  (no regressions vs baseline).
- `./scripts/emit/run.sh --filter=parseError --js-only` — 3/7 pass (was 2/7
  before fix; the new pass is `parseErrorDoubleCommaInCall`, no other tests
  changed).
