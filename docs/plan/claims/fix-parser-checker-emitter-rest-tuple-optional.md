# fix(parser,checker,emitter): emit `[...?T]` for rest tuple with optional inner

- **Date**: 2026-04-26
- **Branch**: `fix/parser-checker-emitter-rest-tuple-optional`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (JS Emit And Declaration Emit Pass Rate)

## Intent

The (invalid) tuple form `[...T?]` is parsed by tsc as a rest element wrapping
an optional inner type and printed as `[...?T]` in declaration emit. tsz was
mishandling it across three layers, producing output like `[...string?: ]` or
`[...string?]`. This change fixes the parser to detect and represent the
pattern, the checker to flatten the `OPTIONAL_TYPE`-over-`REST_TYPE` shape into
a single tuple element with both flags set, and the declaration emitter to
print the canonical `[...?T]` form. Unblocks `restTupleElements1` in the
declaration-emit suite.

## Files Touched

- `crates/tsz-parser/src/parser/state_types.rs` (~25 LOC)
- `crates/tsz-checker/src/types/type_node.rs` (~15 LOC)
- `crates/tsz-emitter/src/declaration_emitter/type_emission.rs` (~14 LOC)
- `crates/tsz-emitter/src/declaration_emitter/tests/probes_issues.rs` (~17 LOC, regression test)

## Verification

- `cargo nextest run -p tsz-parser --lib` (666 tests pass)
- `cargo nextest run -p tsz-checker --lib` (2854 tests pass)
- `cargo nextest run -p tsz-emitter --lib` (1607 tests pass)
- `scripts/emit/run.sh --filter restTupleElements1` (DTS now passes)
