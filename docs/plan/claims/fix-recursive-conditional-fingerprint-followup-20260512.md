# fix(checker): align recursive conditional fingerprint drift

- **Date**: 2026-05-12
- **Branch**: `fix/recursive-conditional-fingerprint-followup-20260512`
- **Base**: `main`
- **PR**: https://github.com/mohsen1/tsz/pull/5762
- **Status**: ready
- **Issue**: #5579
- **Workstream**: conformance

## Intent

Close the reopened `recursiveConditionalTypes.ts` fingerprint-only drift. The
previous recursive conditional slice made the test pass at the time, but the
issue was reopened because current `main` still emits the expected diagnostic
codes with mismatched line, column, or message fingerprints.

## Scope

- Reproduce the current focused conformance delta for
  `recursiveConditionalTypes`.
- Fix the smallest checker/solver diagnostic-boundary root cause needed to
  align fingerprints without changing unrelated recursive conditional
  semantics.
- Add or update focused regression coverage in the owning crate when the root
  cause is isolated.

## Files Touched

- `crates/tsz-checker/src/diagnostics/message_rewriting.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`
- `crates/tsz-solver/src/operations/conditional.rs`
- `docs/plan/claims/fix-recursive-conditional-fingerprint-followup-20260512.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose`
- Focused Rust regression test for the touched checker or solver path
- `cargo fmt --all`
- Broader checker/solver validation if the fix changes shared conditional,
  tuple, indexed-access, or diagnostic display behavior

## Progress

- Claim created.
- Fixed recursive conditional fingerprint drift by preserving scoped bare type
  parameter display in TS2322 message rewriting and anchoring the recursive
  wrapper TS2589 at the conditional alias application use site.

## Verification

- `cargo fmt --all`
- `./scripts/conformance/conformance.sh run --filter "recursiveConditionalTypes" --verbose`
  - `FINAL RESULTS: 2/2 passed (100.0%)`
- `cargo test -p tsz-checker --test conditional_infer_tests recursive_awaited -- --nocapture`
  - `2 passed`
