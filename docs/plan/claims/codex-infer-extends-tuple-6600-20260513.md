# fix(solver): preserve constrained infer captures

- **Date**: 2026-05-13
- **Branch**: `codex/infer-extends-tuple-6600-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / solver false positives

## Intent

Fix #6600 by teaching conditional-type inference to keep successful
`infer T extends Constraint` captures instead of collapsing them to `never`.
The slice targets tuple, function-return, and object-property constrained
infer patterns with focused regression coverage.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/*` (expected)
- `crates/tsz-checker/tests/infer_extends_constraint_substitution_tests.rs` (expected)
- `docs/plan/claims/codex-infer-extends-tuple-6600-20260513.md`

## Verification

- Pending.
