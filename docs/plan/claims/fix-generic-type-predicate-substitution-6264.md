# fix(checker): substitute generic type predicates during narrowing

- **Date**: 2026-05-13
- **Branch**: `fix-generic-type-predicate-substitution-6264`
- **PR**: #6280
- **Status**: ready
- **Workstream**: issue #6264 type-inference false positive

## Intent

Fix #6264, where calls to generic type-predicate functions with explicit type arguments narrow to unsubstituted or unknown predicate types instead of concrete types like `number[]`. The implemented slice preserves function-shape type predicates while applying explicit type arguments, so the existing signature-instantiation path can substitute predicate return types before flow narrowing consumes them.

## Files Touched

- `crates/tsz-checker/src/state/type_resolution/constructors.rs` (~1 LOC behavior fix)
- `crates/tsz-checker/tests/control_flow_type_guard_tests.rs` (focused regression)

## Verification

- `cargo test -p tsz-checker --test control_flow_type_guard_tests explicit_type_argument_instantiates_generic_type_predicate -- --nocapture` (1 passed)
- `cargo test -p tsz-checker --test control_flow_type_guard_tests test_generic_type_predicate_false_branch_does_not_collapse_to_never -- --nocapture` (1 passed)
- `cargo run -p tsz-cli --bin tsz -- --noEmit --strict /tmp/issue6264.ts` (pass)
- `cargo fmt --all -- --check` (pass)
- `git diff --check` (pass)
