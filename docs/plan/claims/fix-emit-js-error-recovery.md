# fix(parser): keep outer prefix `++`/`--` with missing operand in error recovery

- **Date**: 2026-04-26
- **Branch**: `fix/emit-js-error-recovery`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS Emit Pass Rate)

## Intent

When the parser encounters `++ delete foo.bar`, `++ ++ y`, or
`++\n++\ny`, tsc keeps the outer `++`/`--` as a `PrefixUnaryExpression`
with a missing operand and lets the inner unary (`delete …`, `++y`,
…) start the next statement. The JS emitter prints the bare `++;`
followed by the inner expression statement, e.g.:

```js
// expected baseline
++;
delete foo.bar;
```

tsz's prior recovery dropped the outer `++` entirely (returning the
inner unary as the recovered expression), so the JS emit produced
just `delete foo.bar;` and missed the `++;` line. Fix the parser to
report TS1109 at the offending token, build a `PrefixUnaryExpression`
with `operand: NodeIndex::NONE`, and leave the offending token
unconsumed so the next statement parses naturally — matching tsc's
`parsePrimaryExpression` -> `parseIdentifier(Diagnostics.Expression_expected)`
flow.

This flips two error-recovery emit failures
(`parserUnaryExpression5`, `parserS7.9_A5.7_T1`) without regressing
any other parser/emit/conformance test.

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions.rs` (~30 LOC change in
  `parse_unary_expression` recovery branch)
- `crates/tsz-parser/tests/parser_unit_tests.rs` (~120 LOC: rewrite two
  existing recovery tests to assert the corrected AST shape, add a new
  Sputnik-shape test)
- `crates/tsz-emitter/Cargo.toml` (register new test binary)
- `crates/tsz-emitter/tests/prefix_unary_recovery_tests.rs` (new file,
  ~80 LOC: 3 emitter regression tests covering delete-after-update,
  update-after-update, and the Sputnik scenario)

## Verification

- `cargo nextest run -p tsz-parser --tests` (671 tests pass)
- `cargo nextest run -p tsz-emitter --tests` (1656 tests pass, +3 new)
- `scripts/safe-run.sh ./scripts/emit/run.sh --js-only --skip-build`
  → 12330/13526 pass (was 12324, +6 net JS, no regressions)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`
  → 12183/12183 (no delta vs baseline)
