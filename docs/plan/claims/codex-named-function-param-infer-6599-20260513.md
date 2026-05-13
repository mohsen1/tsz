# fix(solver): match named function parameter infer

- **Date**: 2026-05-13
- **Branch**: `codex/named-function-param-infer-6599-20260513`
- **PR**: #6607
- **Status**: ready
- **Workstream**: conformance / solver false positives

## Intent

Fix #6599 by making conditional function-parameter inference ignore parameter
names and correctly match trailing array rest patterns against the remaining
source parameters. The slice targets named parameter `infer` patterns such as
`(first: infer F, ...args: any[]) => any`.

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/infer_pattern_helpers.rs`
- `crates/tsz-checker/tests/infer_extends_constraint_substitution_tests.rs`
- `docs/plan/claims/codex-named-function-param-infer-6599-20260513.md`

## Verification

- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests named_ -- --nocapture` (3 passed)
- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture` (19 passed)
- `cargo fmt --all --check`
- `cargo test -p tsz-solver infer -- --nocapture` (940 passed, 5 ignored)
