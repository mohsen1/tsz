# fix(solver): make TS2739 missing-property sort total

- **Date**: 2026-05-02
- **Branch**: `fix/ts2739-total-order-comparator`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 5 (large-repo fixture stability)

## Intent

Fix the TS2739/TS2741 missing-property ordering comparator so distinct
properties never compare equal when their declaration-order metadata ties.
The large-repo RSS sample exposed a Rust sort panic in this path after #2146
started ordering missing properties by declaration order.

## Planned Scope

- `crates/tsz-solver/src/relations/subtype/explain.rs`
- Focused regression coverage if there is a compact existing test seam
- `docs/plan/claims/fix-ts2739-missing-property-sort-total-order.md`

## Verification

- Pending
