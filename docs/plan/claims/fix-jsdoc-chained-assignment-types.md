# fix(checker): preserve JSDoc chained-assignment types

- **Date**: 2026-05-05
- **Branch**: `fix/jsdoc-chained-assignment-types`
- **PR**: #2787
- **Status**: ready
- **Workstream**: 1 (Conformance - JSDoc chained-assignment diagnostics)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/jsdoc/jsdocTypeFromChainedAssignment.ts`,
a fingerprint-only failure with matching `TS2322`, `TS2339`, and `TS2345`
codes. The live mismatch shows `tsz` loses the chained prototype member type
for `a.z(...)` and renders the static chained-assignment function's `this`
type as `g` instead of `typeof A`.

The root cause was split across two JS constructor paths:

- chained prototype function assignments such as
  `A.prototype.y = A.prototype.z = function f(n) { ... }` only collected the
  outer prototype target, so `z` was not present on `new A()`;
- diagnostic display for `this` inside a function expression assigned through
  `A.t = function g(...) { ... }` fell back to the function value name `g`
  instead of the static receiver `typeof A`.

## Files Touched

- `crates/tsz-checker/src/types/computation/complex_constructors.rs`
- `crates/tsz-checker/src/error_reporter/properties.rs`
- `crates/tsz-checker/tests/js_constructor_property_tests.rs`

## Verification

- `cargo nextest run --package tsz-checker --test js_constructor_property_tests test_jsdoc_chained_prototype_and_static_function_assignments_preserve_member_types` (1 passed)
- `./scripts/conformance/conformance.sh run --filter "jsdocTypeFromChainedAssignment" --verbose` (3/3 passed; target moved from fingerprint-only failure to pass)
- `cargo nextest run --package tsz-checker --test js_constructor_property_tests` (66 passed)
- `cargo nextest run --package tsz-checker --test ts2683_tests js_constructor_function_with_prototype_no_ts2683` (1 passed)
- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (12444/12582 passed; net +7 vs checked-in baseline, including `jsdocTypeFromChainedAssignment.ts` as `FAIL -> PASS`)

Full-suite note: the checked-in baseline diff also listed five `PASS -> FAIL`
entries (`dynamicNames.ts`, `noImplicitAnyIndexing.ts`,
`jsDeclarationsTypeAliases.ts`, `typedefTagTypeResolution.ts`, and
`noUncheckedIndexedAccess.ts`). The same filtered failures reproduce in a
pristine `origin/main` worktree, so they are baseline drift rather than
regressions from this branch.
