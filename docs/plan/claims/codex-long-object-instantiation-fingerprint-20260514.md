# Claim: long object instantiation fingerprint conformance

Status: ready
Owner: Codex
Date: 2026-05-14
PR: #6701

## Intent

Investigate and fix the remaining compiler-area conformance failure for `longObjectInstantiationChain3.ts`, where expected and actual codes both contain `TS2339` but the snapshot still records a failure.

## Scope

- Diagnose whether the mismatch is diagnostic fingerprint, crash/timeout accounting, or conformance runner normalization.
- Fix the root cause with focused validation.
- Keep this separate from PR #6685 parse-recovery cleanup.

## Validation

- `TSZ_LIB_DIR=/Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/lib ./scripts/conformance/conformance.sh run --filter longObjectInstantiationChain3 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/tests/cases --workers 1 --verbose`
  - Result: `FINAL RESULTS: 1/1 passed (100.0%)`; skipped 0; crashed 0; timeout 0; fingerprint-only 0.
- `cargo test -p tsz-checker ts2339_ -- --nocapture`
  - Result: passed. Includes the #6685 CI-failing unit surfaces `ts2339_merge_function_receiver_widens_fresh_object_literals`, `test_ts2339_keeps_conditional_merge_receiver_branch_display`, and `test_ts2339_elides_long_merge_receiver_instantiation_chain`.
