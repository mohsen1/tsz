# fix(solver): preserve constrained infer captures

- **Date**: 2026-05-13
- **Branch**: `codex/infer-extends-tuple-6600-20260513`
- **PR**: #6601
- **Status**: ready
- **Workstream**: conformance / solver false positives

## Intent

Fix #6600 by teaching conditional-type inference to keep successful
`infer T extends Constraint` captures instead of collapsing them to `never`.
The slice targets tuple, function-return, and object-property constrained
infer patterns with focused regression coverage.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern.rs`
- `crates/tsz-checker/tests/infer_extends_constraint_substitution_tests.rs`
- `docs/plan/claims/codex-infer-extends-tuple-6600-20260513.md`

## Verification

- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests test_constrained_infer_ -- --nocapture` (3 passed)
- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture` (19 passed)
- `cargo fmt --all --check`
- `cargo test -p tsz-solver infer -- --nocapture` (940 passed, 5 ignored)
