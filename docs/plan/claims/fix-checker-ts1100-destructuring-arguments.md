# fix(checker): emit TS1100 for `arguments`/`eval` in destructuring patterns

- **Date**: 2026-04-26
- **Branch**: `fix/checker-ts1100-destructuring-arguments`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance fingerprint parity (TS1100 in strict mode)

## Intent

In strict mode, tsc emits TS1100 ("Invalid use of 'arguments' in strict mode")
when `arguments` (or `eval`) appears as a binding name inside a destructuring
pattern, e.g. `var { arguments } = ...`. tsz currently only checks the simple
`var arguments` case in `state/variable_checking/core.rs`, missing identifiers
inside object/array binding patterns. Add the TS1100 check inside
`check_binding_element_with_request` (next to the existing TS1212/1213/1214
binding-element check) so destructuring patterns are covered. This brings the
conformance test `emitArrowFunctionWhenUsingArguments17_ES6` to fingerprint
parity with tsc.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/core.rs` (~25 LOC: emit TS1100/TS1210 for binding-element identifiers named `arguments`/`eval`)
- `crates/tsz-checker/tests/ts1100_destructuring_arguments_tests.rs` (new regression test)

## Verification

- `./scripts/conformance/conformance.sh run --filter "emitArrowFunctionWhenUsingArguments17_ES6" --verbose` → expected pass
- `cargo nextest run -p tsz-checker --test ts1100_destructuring_arguments_tests`
- Targeted no-regression: filter on `ts1100`, `arguments`, `destructuring`
