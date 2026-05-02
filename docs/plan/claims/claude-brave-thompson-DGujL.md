# fix(checker): align TS2345 display with tsc for instanceof-narrowed unions and unbound generic params

- **Date**: 2026-05-02
- **Branch**: `claude/brave-thompson-DGujL`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Conformance — TS2345 fingerprint parity

## Intent

Fix two checker-side display divergences from `tsc` that combined to break
`narrowingGenericTypeFromInstanceof01.ts` (and any test of the same shape):

1. When a flow-narrowed identifier is used as a call argument, the source
   display in TS2345 must use the narrowed type, not the declared union.
   The existing `expr_is_strictly_narrower` check used only `is_assignable_to`,
   which fails when one of the eliminated union members is structurally
   assignable to the surviving member (e.g. `class A { private a } | class B {}`
   narrowed by `instanceof B`: `A` is structurally assignable to the empty
   `B`, so `A | B` is "assignable to" `B` and `B` was treated as not strictly
   narrower). Add a structural "strict union-member subset" path so narrowing
   that eliminates members is recognised regardless of structural overlap.

2. When a generic call's unconstrained type parameter cannot be inferred,
   the parameter display in TS2345 must show the substituted form
   (`A<unknown>`), matching `tsc`. The previous code in `call_finalize.rs`
   actively reverted the substituted parameter type back to the raw signature
   form (`A<T>`) when the substitution had defaulted T to UNKNOWN. Drop that
   override so the substituted form flows through to the diagnostic.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
  (~40 LOC; new `is_strict_union_member_subset` helper, wired into
  `declared_identifier_source_display` narrower check).
- `crates/tsz-checker/src/types/computation/call_finalize.rs`
  (~25 LOC removed; `reported_expected_param = expected_param`).
- `crates/tsz-checker/Cargo.toml` (new test entry).
- `crates/tsz-checker/tests/ts2345_instanceof_narrowed_union_display_tests.rs`
  (new; 3 tests covering non-generic, generic, and renamed generics).

## Verification

- `cargo test -p tsz-checker --test ts2345_instanceof_narrowed_union_display_tests`
  (3/3 pass).
- `cargo test -p tsz-checker --test ts2339_union_narrow_display_tests`
  (existing narrowing-display tests still pass).
- `scripts/safe-run.sh cargo clippy --workspace --all-targets --all-features -- -D warnings`
  (clean).
- `cargo fmt --all --check` (clean).
- `./scripts/conformance/conformance.sh run --filter narrowingGenericTypeFromInstanceof01 --verbose`
  (1/1 PASS — this was the picked random failure).
- 200-test conformance smoke (`tsz-conformance --max 200`): 200/200 pass.
- Full conformance suite: see PR body for net change.

## Structural Rule

> When `tsc` displays a TS2345 diagnostic for a call argument whose identifier
> was flow-narrowed to a strict subset of its declared union's members,
> `tsz` must display the narrowed type as the argument and the substituted
> parameter type (with `unknown` for unbound type parameters) as the
> parameter — even when one of the eliminated union members is structurally
> assignable to the surviving member, and even when generic inference left a
> type parameter unbound.

The fix is structural (operates over `TypeId` union members and the
solver's `final_subst` substitution), not name- or printer-output-based:
it does not regex over rendered display text and does not key off
identifier names, so it does not depend on the user choosing `T`/`P`/`X`
for type parameters or the surviving union member's class name.
