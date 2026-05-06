# fix(checker): align styled-components TS2344 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next23`
- **PR**: #3715
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/styledComponentsInstantiaionLimitNotReached.ts`.

Current `origin/main` emits `TS2344`, but the fingerprints differ from tsc:

- Missing `TS2344` at `test.ts:172:39` for `WithC`.
- Missing `TS2344` at `test.ts:195:65` for `AnyStyledComponent & C`.
- Extra `TS2344` at `test.ts:91:21` for the conditional `C extends ... ? C : never`.

This slice aligns the generic constraint diagnostics without broadening TS2344
suppression for ordinary type argument constraint failures.

## Files Touched

- `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
- `crates/tsz-checker/src/error_reporter/generics.rs`
- `crates/tsz-checker/src/state/state_checking_members/interface_checks.rs`
- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
- `crates/tsz-checker/tests/ts2344_infer_conditional_constraint.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "styledComponentsInstantiaionLimitNotReached" --verbose` (1/1 passed)
- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker test_react_component_props_with_ref_accepts_conditional_element_type test_styled_component_inner_component_constraint_errors_at_declaration_time` (2 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/architecture-check.sh --quick` (exited 0; existing LOC warnings only)
- `CARGO_TARGET_DIR=.target/nextest-local cargo clippy -p tsz-checker --lib -- -D warnings`
