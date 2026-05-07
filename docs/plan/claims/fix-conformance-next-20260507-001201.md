# [WIP] fix(checker): align recursive conditional conformance diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-001201`
- **PR**: https://github.com/mohsen1/tsz/pull/4309
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

The canonical picker selected
`TypeScript/tests/cases/compiler/recursiveConditionalTypes.ts`. On current
`origin/main`, the filtered run remains an XFAIL with expected
`[TS2322, TS2345, TS2589]` and actual
`[TS2322, TS2339, TS2345, TS2536, TS2589]`.

This slice will root-cause the remaining recursive conditional diagnostic gap:
missing/deviating `TS2589`, `TS2322`, and `TS2345` fingerprints, plus extra
`TS2339` and `TS2536` diagnostics around recursive tuple/array conditional
evaluation.

## Planned Scope

- Solver/checker conditional, tuple, indexed-access, or diagnostic-boundary
  code as the root cause requires.
- A focused Rust regression test in the owning crate.
- Targeted conformance verification for `recursiveConditionalTypes`.

## Verification Plan

- `cargo fmt --all`
- Focused Rust regression test(s)
- `cargo nextest run -p tsz-checker --lib` and/or `cargo nextest run -p tsz-solver --lib`
  depending on touched crates
- `./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose`
- Architecture guardrails if checker boundary code changes
- Pre-commit hook before publishing ready PR

## Progress

- Removed extra `TS2339`/`TS2536` fingerprints for recursive array-like
  `length` access.
- Suppressed false recursive-alias `TS2322` drift for structurally identical
  conditional type aliases.
- Preserved `Grow1<[], T>` in the recursive conditional call-argument
  `TS2345` target display.

## Verification

- `cargo fmt --all`
- `cargo nextest run -p tsz-checker recursive_conditional_call_parameter_keeps_alias_display structurally_identical_recursive_conditionals_are_assignable nested_tuple_rest_infer_result_satisfies_array_constraint recursive_tuple_spread_length_index_access_is_valid`
- `./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose`
  remains XFAIL with matching diagnostic codes and no extra fingerprints;
  remaining work is the missing tuple assignment, `Enumerate<T["length"]>`
  return, and two `TS2589` fingerprints.
