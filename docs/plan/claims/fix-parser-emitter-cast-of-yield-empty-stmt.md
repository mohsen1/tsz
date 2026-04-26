# fix(parser): keep `<T> yield` as missing-expression + separate yield stmt

- **Date**: 2026-04-26
- **Branch**: `fix/parser-emitter-cast-of-yield-empty-stmt`
- **PR**: #1367
- **Status**: ready
- **Workstream**: 2 (JS Emit Pass Rate)

## Intent

`parse_type_assertion` reported TS1109 when it saw `<number> yield 0` inside
a generator, but then still called `parse_unary_expression`, which consumed
the `yield 0` as the type assertion's inner expression. tsc's
`parseSimpleUnaryExpression` does **not** handle `YieldKeyword`, so tsc
recovers with a missing inner expression and re-parses `yield 0` as a
separate yield expression statement. The mismatch produced one fewer line
of JS output than tsc for the conformance test `castOfYield`.

This change mirrors tsc: when we see `YieldKeyword` after `>` in a
generator, report TS1109 and use `NodeIndex::NONE` for the assertion's
inner expression instead of consuming the yield. The remaining tokens
(`yield 0`) parse normally as the next statement, producing the empty `;`
plus standalone `yield 0;` shape that tsc emits.

## Files Touched

- `crates/tsz-parser/src/parser/state_types_jsx.rs` (~10 LOC: surgical
  guard in `parse_type_assertion` for `YieldKeyword` in generator context)
- `crates/tsz-parser/tests/parser_unit_tests.rs` (regression test
  `type_assertion_does_not_consume_yield_in_generator`)

## Verification

- `cargo nextest run -p tsz-parser` (670 tests pass, +1 new)
- `cargo nextest run -p tsz-emitter` (1650 tests pass, no regressions)
- `cargo nextest run -p tsz-checker --lib` (2878 tests pass)
- `./scripts/conformance/conformance.sh run --filter castOfYield` (1/1 pass)
- `./scripts/conformance/conformance.sh run --filter yield` (107/107 pass)
- `./scripts/conformance/conformance.sh run --filter cast` (9/9 pass)
- `./scripts/conformance/conformance.sh run --filter asyncFunction` (80/80 pass)
- Emit suite: JS pass rate 91.1% → 91.1% with `castOfYield` flipping
  fail → pass (12324 → 12325, +1; total fail 1202 → 1201).
