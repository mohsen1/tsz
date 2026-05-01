# checker: preserve `keyof T` display for free type parameter in array-literal TS2322 messages

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-GGyYn`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint parity)

## Intent

Match `tsc`'s diagnostic display for `keyof T` when `T` is a free type
parameter and an array literal element fails the per-element TS2322 check.
Flips the fingerprint-only failure `compiler/keyofIsLiteralContexualType.ts`.

For `function foo<T extends { a: string; b: string }>()` and
`let b: (keyof T)[] = ["a", "b", "c"];`:

```ts
// Before: Type '"c"' is not assignable to type '"a" | "b"'.
// After (matches tsc):
//   Type 'string' is not assignable to type 'keyof T'.
```

## Root Cause

`evaluate_keyof` for a `TypeParameter` with a non-trivial concrete
constraint (`crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs:295-326`)
collapses `keyof T` to `keyof Constraint(T)` — the literal-keys union
of the constraint's properties. `try_elaborate_array_literal_elements` in
`crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs` ran the
parameter through `evaluate_type_with_env` (which, via `visit_array`,
distributes evaluation into the array's element), so the per-element
diagnostic carried `"a" | "b"` as the target instead of the deferred
`keyof T`.

## Fix

Surgical: keep the evaluated form for the assignability *check*, but anchor
the user-facing TS2322 message on the un-evaluated array element type when
the parameter is a plain `T[]` (as opposed to a tuple, which has explicit
slot types that don't need this preservation). This matches tsc's display
without disturbing the relation logic that consumes the keyed form.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
  (~16 LOC change in `try_elaborate_array_literal_elements`).
- `crates/tsz-checker/tests/keyof_array_literal_diagnostic_tests.rs`
  (new file, 2 regression tests).
- `crates/tsz-checker/Cargo.toml` (register the new integration-test target).

## Verification

- `cargo test -p tsz-checker --test keyof_array_literal_diagnostic_tests` — 2 pass.
- `cargo test -p tsz-checker --test keyof_naked_priority_tests` — 5 pass
  (no regression in the `naked_obj_t_picks_union_of_keys_for_keyof_diagnostic`
  assertion, because that path inferers `T` to a concrete object).
- `cargo test -p tsz-checker --test ts2322_tests` — 141 pass.
- `cargo fmt --check -p tsz-checker` — clean.
- `cargo clippy -p tsz-checker --all-targets --all-features -- -D warnings` — clean.
- `tsz-conformance --filter keyofIsLiteralContexualType` — 1/1 passed.
- `tsz-conformance --filter nodeModulesImportModeDeclarationEmit2` — already
  1/1 passed (snapshot was stale; included here as a sanity drive-by).
