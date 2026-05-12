# fix(audit): restrict TupleOf recursive-alias shortcut to type aliases

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch4-20260512`
- **PR**: #5879
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close a missed important review note from PR #4990: `recursive_tuple_declared_assignment_types` keyed only on `def.name == "TupleOf"`, which also matched non-alias declarations (for example an `interface TupleOf<...>`). That forced the recursive-alias compatibility path for shapes where it should not apply, producing spurious TS2322 in otherwise valid assignment directions.

## Files Touched

- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/tests/conditional_infer_tests.rs`

## Verification

- `cargo test -p tsz-checker --test conditional_infer_tests interface_tupleof_assignment_uses_constraint_directionality -- --exact --nocapture`
- `cargo test -p tsz-checker --test conditional_infer_tests recursive_tuple_alias_assignment_reports_both_directions -- --exact --nocapture`
- `cargo check -p tsz-checker`
