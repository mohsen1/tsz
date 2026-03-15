# Critical Finding: noImplicitAny defaults to true (should be false)

## Issue
Our compiler defaults `noImplicitAny` to `true` (line 524 in `crates/tsz-core/src/lib.rs`).
tsc defaults it to `false`. This affects **9060 tests (72%)** that don't specify `@strict`.

## Impact
- Tests without `@strict` run with `noImplicitAny: true` when they should run with `false`
- This causes false TS7006 ("parameter implicitly has 'any'"), TS2345, TS2322 errors
- Also causes `[]` to be typed as `never[]` in chained access patterns instead of `any[]`

## Location
- `crates/tsz-core/src/lib.rs:524`: `self.resolve_bool(self.no_implicit_any, true)`
- Should be: `self.resolve_bool(self.no_implicit_any, false)`
- Same issue for `strictNullChecks` at line 529

## Risk
- HIGH: Changing this affects 72% of all tests
- Will fix many tests but may also cause regressions
- Needs full conformance suite run before and after
- Should be done as a dedicated, carefully-tested change

## Recommendation
1. Run full conformance suite with current defaults
2. Change defaults to `false` (matching tsc)
3. Run full conformance suite again
4. Compare: if net positive, merge. If regressions, investigate each.
