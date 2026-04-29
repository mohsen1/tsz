# [WIP] fix(checker): align reverse mapped intersection diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/reverse-mapped-intersection-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only diagnostic mismatch for
`reverseMappedTypeIntersectionConstraint.ts`, where TSZ and tsc agree on the
`TS2322` and `TS2353` codes but disagree on diagnostic details. The fix will
identify whether the mismatch is in assignability display, excess-property
reporting, or reverse-mapped/intersection constraint semantics, then change the
owning layer with a focused Rust regression test.

## Files Touched

- `docs/plan/claims/fix-reverse-mapped-intersection-fingerprint.md` (claim)
- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/tests/reverse_mapped_inference_tests.rs`

## Verification

- `cargo check --package tsz-checker` (passes)
- `cargo nextest run -p tsz-checker reverse_mapped_const_generic_ts2353_omits_outer_readonly_in_target_display` (passes)
- `./scripts/conformance/conformance.sh run --test-dir /tmp/tsz-single-tests --filter "reverseMappedTypeIntersectionConstraint" --verbose` (still fingerprint-only; reduced the line 172 outer-readonly mismatch, remaining drift includes nested display and TS2322-vs-TS2353 prioritization)
