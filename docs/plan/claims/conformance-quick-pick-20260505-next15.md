# [WIP] fix(checker): align mapped type relationship diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next15-impl`
- **PR**: #3096
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`mappedTypeRelationships.ts`. The picked failure has matching diagnostic codes,
but several TS2322 messages differ from tsc around indexed access and mapped
type relationship displays.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/src/types/computation/access_helpers.rs`
- `crates/tsz-checker/tests/conformance_issues/types/indexed_access.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs`
- `crates/tsz-solver/src/relations/compat.rs`
- `crates/tsz-solver/src/relations/subtype/rules/generics.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/types/mapped/mappedTypeRelationships.ts`.
- `cargo fmt --check`
- `cargo nextest run -p tsz-solver test_mapped_to_mapped_readonly_partial_t_equiv_partial_readonly_t`
- `cargo nextest run -p tsz-checker --test conformance_issues types::indexed_access::`
- `CARGO_BUILD_JOBS=4 ./scripts/conformance/conformance.sh run --filter "mappedTypeRelationships" --verbose`
