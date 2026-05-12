# test(checker): regression-lock TS2802 for non-array for-of without downlevelIteration

- **Date**: 2026-05-12
- **Branch**: `fix/checker-ts2802-downlevel-iteration-2026-05-12`
- **Base**: `main`
- **Issue**: [#5893](https://github.com/mohsen1/tsz/issues/5893)
- **PR**: [#5940](https://github.com/mohsen1/tsz/pull/5940)
- **Status**: ready (regression-lock; the diagnostic already works on current main)
- **Labels**: `regression-lock`, `checker-tests`

## Intent

Regression-lock for #5893. Current `main` already emits TS2802 for this
scenario; this PR keeps that behavior covered by focused tests. When
targeting ES5/ES3 and `downlevelIteration` is OFF,
tsc emits TS2802 ("Type 'X' can only be iterated through when using
the '--downlevelIteration' flag or with a '--target' of 'es2015' or
higher.") for `for-of` over any iterable other than a plain
array/string/argument-list.

## Structural rule

> When `target < ES2015` AND `downlevelIteration` is OFF AND the
> for-of source type is iterable (implements `[Symbol.iterator]`)
> but is NOT one of the directly-iterable types tsc allows in the
> downlevel emit (Array, String, arguments), the checker emits
> TS2802 at the `for-of` statement's iteree anchor.

(Anti-hardcoding: tests will exercise two distinct iterable class
names — `Range` and a renamed variant — to lock the rule against
identifier-dependent fixes.)

## Files Touched

- `crates/tsz-checker/tests/ts2802_downlevel_iteration_tests.rs` — add
  regression tests and cache default libs for the file.
- `docs/plan/claims/fix-checker-ts2802-downlevel-iteration-2026-05-12.md`
  — keep claim metadata aligned with the tests-only PR.

## Out of scope

- The `downlevelIteration` runtime emitter — only the diagnostic.
- Spread expressions / array destructuring that also produce TS2802
  in tsc — separate follow-ups if not covered by the same gate.

## Verification

- Unit test that locks the new TS2802 emission on the issue's exact
  repro + a renamed variant.
- Regression: `cargo nextest run -p tsz-checker --lib` stays green.
- Targeted conformance smoke for any TS2802 baseline tests.

## Risk

Low. Pure additive regression coverage for existing diagnostic behavior.
