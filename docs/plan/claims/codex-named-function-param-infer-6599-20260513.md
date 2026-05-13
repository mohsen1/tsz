# fix(solver): match named function parameter infer

- **Date**: 2026-05-13
- **Branch**: `codex/named-function-param-infer-6599-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / solver false positives

## Intent

Fix #6599 by making conditional function-parameter inference ignore parameter
names and correctly match trailing array rest patterns against the remaining
source parameters. The slice targets named parameter `infer` patterns such as
`(first: infer F, ...args: any[]) => any`.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs` (expected)
- `crates/tsz-checker/tests/infer_extends_constraint_substitution_tests.rs` (expected)
- `docs/plan/claims/codex-named-function-param-infer-6599-20260513.md`

## Verification

- Pending.
