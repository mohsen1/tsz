# fix(checker): report NaN equality diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-nan-equality-diagnostic`
- **PR**: https://github.com/mohsen1/tsz/pull/3152
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the all-missing conformance failure in
`TypeScript/tests/cases/compiler/nanEquality.ts`. TypeScript reports `TS2845`
for equality comparisons against the global `NaN`, while tsz currently emits no
diagnostic for this target.

This is distinct from the earlier shadowed-`NaN` false-positive fix: local
parameters named `NaN` must remain accepted, but comparisons involving the
global lib `NaN` should be diagnosed.

## Files Touched

- `crates/tsz-checker/src/types/computation/expression_guards.rs`
- `crates/tsz-checker/tests/conformance_issues/features/async.rs`
- `docs/plan/claims/fix-checker-nan-equality-diagnostic.md`

## Verification

- PASS `CARGO_TARGET_DIR=/tmp/tsz-pr3152-verify-target CARGO_INCREMENTAL=0 cargo test -j 2 -p tsz-checker --test conformance_issues nan -- --nocapture`
- NOT RUN `./scripts/conformance/conformance.sh run --filter "nanEquality" --verbose` in the refresh worktree because `TypeScript/tests/cases` is not present, so the harness reports no test files.
