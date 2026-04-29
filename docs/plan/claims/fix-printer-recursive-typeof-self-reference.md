# fix(checker): preserve `typeof X` in self-referential param positions

- **Date**: 2026-04-29
- **Branch**: `fix/printer-recursive-typeof-self-reference`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes) — Tier 1 type-display-parity campaign

## Intent

`normalize_assignability_display_type_inner` evaluates the input type before
walking its shape (`evaluate_type_for_assignability`). For
self-referential `typeof X` in **parameter** positions (e.g. `static g(t:
typeof C.g)` where `C.g`'s value type IS that function), evaluation
substitutes the inner `TypeQuery(C.g)` with the resolved function shape,
producing one extra outer wrapper and rendering as
`(t: (t: typeof g) => void) => void` instead of `(t: typeof g) => void`.

The existing return-type guard in `should_use_evaluated_assignability_display`
prevents the equivalent expansion in **return** positions; this PR adds the
matching guard for parameter positions, plus an early-return in
`normalize_assignability_display_type_inner` when the function/callable
shape carries `TypeQuery` in any param or return — leaving the original
shape intact for the formatter.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/type_display.rs`:
  - extend the function/callable signature guard in
    `should_use_evaluated_assignability_display` to inspect params, not just
    return.
  - early-return in `normalize_assignability_display_type_inner`'s
    final `else` branch when ty is a function/callable whose signature
    contains a TypeQuery.
- `crates/tsz-checker/tests/recursive_typeof_param_display_tests.rs`:
  new regression test asserting `(t: typeof g) => void` is preserved in
  TS2345 messages.
- `crates/tsz-checker/Cargo.toml`: register the new test target.

## Verification

- `cargo nextest run -p tsz-checker --test recursive_typeof_param_display_tests`
  — passes.
- `cargo nextest run -p tsz-checker --lib` — 2960/2960 pass.
- Quick regression `--max 200` — "No regressions or improvements vs baseline".
- Targeted: `recursiveFunctionTypes.ts` flips the line-22 TS2345 fingerprint
  (one of three). The remaining two patterns (line 25 TS2322 expecting
  `() => ...` and line 34 TS2345 expecting an unwrapped overload set) need
  separate, distinct policies and are out of scope here.
- Full conformance run pending before flipping `Status: ready`.
