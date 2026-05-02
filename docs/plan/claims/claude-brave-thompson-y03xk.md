# fix(checker): emit TS2345 for unrelated bare type-parameter arg/param mismatches

- **Date**: 2026-05-02
- **Branch**: `claude/brave-thompson-y03xk`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — TS2345 false negative)

## Intent

`should_defer_contextual_argument_mismatch` deferred any `ArgumentTypeMismatch`
where both `actual` and `expected` "contain type parameters", regardless of
whether either side was actually in flight as an inference variable. For two
*bare* enclosing-scope `TypeData::TypeParameter` types — e.g. `U` passed
to a parameter typed `T` from `function foo<T, U>(...)` — the deferral
silently dropped a real TS2345 because no further inference can ever change
either side.

This PR adds a structural exit from the deferral branch: when both `actual`
and `expected` are bare named `TypeData::TypeParameter` (excluding `Infer`
and `BoundParameter`) with different identities, return `false` so the
checker emits the diagnostic.

Targets `compiler/genericCallbackInvokedInsideItsContainingFunction1.ts`
which moves from fingerprint-only (one missing `TS2345 test.ts:12:17 'U' is
not assignable to 'T'`) to PASS.

## Files Touched

- `crates/tsz-solver/src/type_queries/core.rs` (+16): new
  `is_bare_named_type_parameter` query helper (excludes Infer / BoundParameter).
- `crates/tsz-checker/src/query_boundaries/checkers/generic.rs` (+8): boundary
  wrapper.
- `crates/tsz-checker/src/types/computation/call_result.rs` (+21): bare
  type-param early-exit in `should_defer_contextual_argument_mismatch`.
- `crates/tsz-solver/tests/type_parameter_comprehensive_tests.rs` (+94):
  solver-level tests for the helper and for `is_subtype_of(U, T) == false`.
- `crates/tsz-checker/tests/generic_tests.rs` (+86): four checker-level
  TS2345 tests (T/U pair, P/Q rename, reflexive T->T, constrained `U extends T`).

## Verification

- `cargo test -p tsz-checker --lib` — 3128 pass, 0 fail.
- `cargo test -p tsz-solver --lib` — 5589 pass, 0 fail.
- `cargo fmt --all --check` clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- Targeted conformance: `genericCallbackInvokedInsideItsContainingFunction1`
  flips to PASS (1/1 100%).
- `--max 200` regression smoke 200/200 PASS.
- No newly-failing `typeParameter*` conformance tests; the three failures
  observed in that filter were already failing on the snapshot before the
  fix.
