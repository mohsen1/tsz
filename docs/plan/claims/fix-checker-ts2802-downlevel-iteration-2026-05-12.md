# fix(checker): emit TS2802 for non-array for-of without downlevelIteration

- **Date**: 2026-05-12
- **Branch**: `fix/checker-ts2802-downlevel-iteration-2026-05-12`
- **Base**: `main`
- **Issue**: [#5893](https://github.com/mohsen1/tsz/issues/5893)
- **Status**: claim
- **Labels**: `bug`, `missing-diagnostic`

## Intent

Closes #5893. When targeting ES5/ES3 and `downlevelIteration` is OFF,
tsc emits TS2802 ("Type 'X' can only be iterated through when using
the '--downlevelIteration' flag or with a '--target' of 'es2015' or
higher.") for `for-of` over any iterable other than a plain
array/string/argument-list. tsz does not currently emit this.

## Structural rule

> When `target < ES2015` AND `downlevelIteration` is OFF AND the
> for-of source type is iterable (implements `[Symbol.iterator]`)
> but is NOT one of the directly-iterable types tsc allows in the
> downlevel emit (Array, String, arguments), the checker emits
> TS2802 at the `for-of` statement's iteree anchor.

(Anti-hardcoding: tests will exercise two distinct iterable class
names — `Range` and a renamed variant — to lock the rule against
identifier-dependent fixes.)

## Files Touched (estimated)

- `crates/tsz-checker/src/state/state_checking/` (the for-of /
  statement-checking site) — add the diagnostic gate.
- New test in `crates/tsz-checker/tests/ts2802_downlevel_iteration_tests.rs`.

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

Low. Pure additive diagnostic gated by compiler options
(`target < ES2015` && `!downlevelIteration`). No semantic change
for the supported configs.
