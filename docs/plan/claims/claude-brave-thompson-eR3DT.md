# fix(checker): compose round-1 inferred type params with return-context substitution in overload retry

- **Date**: 2026-05-02
- **Branch**: `claude/brave-thompson-eR3DT`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / TS2769 false-positive

## Intent

Two-argument generic-method overloads on a generic shape like
`call<T, U>(xs: ArrayLike<T>, mapfn: (v: T, k: number) => U): U[]` fail
to type the callback body when the call site has both a non-callback
argument that fixes `T` and an outer contextual return type that fixes
`U`. The overload-retry path used either the round-1 substitution
(when no return-context binding) or the return-context substitution (when
it was non-empty) — but never both. As a result, when the contextual
return type fired, `T` was discarded, the callback parameter became
`(v: T, k: number) => A` with `T` unresolved, and tsz emitted spurious
`TS2339` / `TS2769` over an otherwise valid call.

The fix composes the two: the return-context substitution is applied on
top of the round-1 inferred parameter list rather than replacing it.
This unblocks the `arrayFrom` conformance test (formerly XFAIL) and
removes its `PRODUCTION_SUPPRESSION_DEBT_PATTERNS` entry.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
  (three sibling sites: pass-1 retry, pass-2 preinfer, pass-2 retry)
- `crates/tsz-checker/tests/call_resolution_regression_tests.rs`
  (two new locking tests)
- `crates/conformance/src/runner.rs`
  (drop the now-passing `arrayFrom` known-debt entry)

## Verification

- `cargo nextest run -p tsz-checker --lib` (3124 tests pass)
- `cargo nextest run -p tsz-solver --lib` (5579 tests pass)
- `cargo nextest run -p tsz-checker --test call_resolution_regression_tests`
  (138 tests pass; 2 new tests `overload_with_outer_contextual_*`)
- `./scripts/conformance/conformance.sh run --filter arrayFrom --verbose`
  → `2/2 passed (100%)` (was `1/2`, with `arrayFrom.ts` XFAIL)
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features --profile ci-lint -- -D warnings`
- Full conformance suite (no regressions) — see PR body for delta.
