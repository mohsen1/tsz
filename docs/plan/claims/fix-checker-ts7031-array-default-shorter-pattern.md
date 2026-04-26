# fix(checker): emit TS7031 for array binding leaves beyond array-literal default

- **Date**: 2026-04-26
- **Branch**: `fix/checker-ts7031-array-default-shorter-pattern`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fingerprint parity)

## Intent

For `function f([x, y] = [1]) {}`, tsc reports TS7031 for `y` because the
array-literal default `[1]` only covers index 0; `y` at index 1 has no
contextual coverage and no own default, so it stays implicitly `any`.
`tsz` previously short-circuited TS7031 emission whenever the outer
initializer was any non-empty array literal, missing this case.

This PR refines the implicit-any pattern check so that a non-empty
array-literal default still triggers TS7031 for binding leaves at indices
the literal does not cover (excluding leaves with their own default and
nested patterns, which are recursed). Spread elements in the default keep
the existing skip behavior since their effective length is not statically
known.

Target conformance test:
`conformance/es6/destructuring/destructuringWithLiteralInitializers2.ts`
(8 expected fingerprints; we previously emitted only 6).

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`
  (~150 LOC: 1 helper `array_literal_init_len`, 1 emitter
  `emit_implicit_any_for_array_pattern_beyond_default`, 1 dispatch branch,
  6 unit tests).

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2857 passed, 0 failed.
- `cargo nextest run -p tsz-checker --lib -E 'test(implicit_any)'` — 45 passed.
- 6 new unit tests under
  `state_domain::state_checking_members::implicit_any_checks::tests` pin
  the new behavior (`ts7031_emitted_for_array_pattern_index_beyond_array_default`,
  `_with_inner_default`, `no_ts7031_when_array_default_covers_pattern`,
  `no_ts7031_when_inner_default_present_beyond_array_default`,
  `ts7031_for_each_uncovered_index_in_longer_pattern`,
  `no_ts7031_for_array_pattern_with_spread_default`).
- Targeted CLI run on `destructuringWithLiteralInitializers2.ts`-shaped input
  now emits 8 TS7031 fingerprints matching tsc.
