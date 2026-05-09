# fix(printer): preserve literal type-arg display in TS2322/TS2345 messages

- **Date**: 2026-05-09
- **Branch**: `fix/ts2322-preserve-literal-display-2026-05-09`
- **PR**: TBD (will draft as WIP)
- **Status**: claim
- **Workstream**: type-display-parity (Tier 1 fingerprint campaign)

## Intent

tsz currently widens literal type-arguments inside generic applications when
formatting TS2322 / TS2345 messages, while tsc preserves them. For example
`max2(1, 2)` against `<T extends Comparable<T>>(x: T, y: T): T` reports:

- tsc: `Argument of type 'number' is not assignable to parameter of type 'Comparable<1 | 2>'.`
- tsz: `Argument of type 'number' is not assignable to parameter of type 'Comparable<number>'.`

The asymmetry tsc applies — widen the *outer* arg, but preserve literal
type-args *nested* in a generic application — also shows up in
`objectLiteralNormalization` (`{ c: true; }` vs `{ c: boolean; }` for the
inferred fresh-object type printed in the assignment-failure message).

The fix will isolate the literal-widening to the outer assignability
display only, leaving display of generic application arguments structural.

## Targeted tests

- `compiler/maxConstraints.ts` (TS2345, single fingerprint diff)
- `compiler/objectLiteralNormalization.ts` (TS2322, partial — 1 of 4 diffs)
- (others will be discovered during investigation)

## Files Touched (planned)

- `crates/tsz-solver/src/diagnostics/format/mod.rs` (display of `TypeData::Application` arg list)
- `crates/tsz-checker/src/error_reporter/...` (call-site decisions)
- new unit tests in `tsz-solver` and/or `tsz-checker`

## Verification

- `cargo nextest run -p tsz-solver --lib` clean
- `cargo nextest run -p tsz-checker --lib` clean
- `./scripts/conformance/conformance.sh run --filter <test> --verbose` flips
- Snapshot regen `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot` net-positive
