# fix(checker): require Symbol.iterator for ES2015+ destructuring (TS2488)

- **Date**: 2026-04-26
- **Time**: 2026-04-26 19:50:00
- **Branch**: `fix/conformance-quick-pick-1777225619`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

Conformance test
`TypeScript/tests/cases/conformance/es6/destructuring/destructuringArrayBindingPatternAndAssignment2.ts`
expects TS2488 (`Type 'F' must have a '[Symbol.iterator]()' method that returns
an iterator.`) for `var [c4, c5, c6] = foo(1)` where `foo` returns
`interface F { [idx: number]: boolean }`. Tsc requires actual `[Symbol.iterator]`
support for ES2015+ destructuring; a numeric index signature is not enough.

`crates/tsz-checker/src/checkers/iterable_checker.rs::check_destructuring_iterability`
had a lenient short-circuit that returned `true` (no TS2488) whenever the
resolved type had a numeric index signature, even on ES2015+ targets. That
short-circuit is incorrect once the ES5 path above (`target.is_es5()`) has
already handled the legacy case. Removing the lenient branch lets ES2015+
destructuring fall through to `emit_ts2488_not_iterable` for index-signature-only
types, matching tsc.

## Root cause

`is_iterable_type` in the same file already covers Array/Tuple/StringLiteral,
union/intersection, classified objects with explicit Symbol.iterator, and
type-parameter constraints. The numeric-index-signature relaxation was a
generic compatibility shim that should never have applied past the ES5
fast-path. Tsc emits TS2488 for these types regardless of index signatures.

## Files Touched

- `crates/tsz-checker/src/checkers/iterable_checker.rs` — drop the
  `has_numeric_index_signature(resolved_type)` short-circuit at the end of
  `check_destructuring_iterability`. The ES5 path already handles legacy
  array-like destructuring before reaching this branch.
- `crates/tsz-checker/tests/spread_rest_tests.rs` — add
  `test_destructuring_index_signature_only_emits_ts2488_in_es2015` regression
  test pinning the new behavior.

## Verification

- `cargo nextest run --package tsz-checker --lib` — 2894 unit tests, all pass.
- `cargo nextest run --package tsz-checker -E 'test(test_destructuring_index_signature)'` — new test passes.
- `./scripts/conformance/conformance.sh run --filter "destructuringArrayBindingPatternAndAssignment2" --verbose`
  — TS2488 now emitted at line 35:5 (`F` not iterable). Two unrelated TS2488
  fingerprints (3:6 and 3:12, nested empty-array destructure on `undefined`)
  remain, but those involve nested-pattern-on-empty-array iteration which is a
  separate root cause.
