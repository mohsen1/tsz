# fix(checker): mixed enum reverse lookup emits false TS7053

- **Date**: 2026-05-12
- **Branch**: `claude/busy-knuth-1yQJQ`
- **PR**: TBD
- **Status**: ready
- **Workstream**: checker/solver correctness

## Intent

Mixed enums (e.g., `enum E { A = 0, B = "B" }`) have numeric members that
generate a runtime reverse mapping (`E[0] === "A"`), but `enum_object_type`
only added `[index: number]: string` for `EnumKind::Numeric`, not
`EnumKind::Mixed`. This caused a false TS7053 on `Mixed[0]`. The fix extends
the condition to cover both kinds. A companion fix in `keyof.rs` prevents the
new index signature from leaking into `keyof typeof E`.

## Files Touched

- `crates/tsz-checker/src/state/type_environment/core.rs` — extend numeric index signature to mixed enums
- `crates/tsz-checker/src/declarations/namespace_checker.rs` — same for namespace object type
- `crates/tsz-checker/src/state/state.rs` — add `EnumKind::has_reverse_mapping()`
- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs` — exclude enum namespace from number keyof
- `crates/tsz-checker/tests/enum_nominality_tests.rs` — 4 new regression tests
