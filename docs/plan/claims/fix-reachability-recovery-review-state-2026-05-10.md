# fix(parser): tighten reachability recovery state

- **Date**: 2026-05-10
- **Branch**: `fix/reachability-recovery-review-state-2026-05-10`
- **PR**: #4971
- **Status**: in review
- **Workstream**: parser-conformance

## Intent

Follow up on review feedback from the reachability recovery parser fix.
The parser's recovery state now clears the pending `const` binding-name
colon hint when a non-simple binding token appears, and the `for` expression
comma recovery diagnostic is emitted only once for the same expression.

The tests cover the broader parser-state behavior instead of only the
original conformance fixture.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class.rs`
- `crates/tsz-parser/tests/state_statement_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo test -p tsz-parser --lib definite_assignment_recovery -- --nocapture`
- `cargo test -p tsz-parser --lib parse_definite_assignment_marker_return_type_reports_statement_recovery -- --nocapture`
