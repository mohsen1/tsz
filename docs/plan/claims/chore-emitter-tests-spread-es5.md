# chore(emitter/tests): expand unit tests for spread_es5 transformer

- **Date**: 2026-04-26
- **Branch**: `chore/emitter-tests-spread-es5`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8 (test coverage gaps in tsz-emitter transforms)

## Intent

Pure-additive unit tests for `crates/tsz-emitter/src/transforms/spread_es5.rs`,
which exposes the `ES5SpreadTransformer` API for lowering ES6 spread operators
to ES5. The current test file has only 2 tests covering construction; the
public detection helpers (`array_contains_spread`, `call_contains_spread`,
`object_contains_spread`) and transform helpers (`transform_array_spread`,
`transform_call_spread`, `transform_object_spread`, `transform_new_spread`)
are not directly unit-tested. This PR fills that gap by parsing minimal TS
fragments and asserting helper behavior on the resulting arena nodes.

Behavior-preserving — adds tests only.

## Files Touched

- `crates/tsz-emitter/tests/spread_es5.rs` (~250 LOC of new tests)

## Verification

- `cargo nextest run -p tsz-emitter -E 'test(spread_es5)'`
