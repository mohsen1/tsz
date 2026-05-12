# fix-as-const-excess-property-ts2353-20260512

Status: claim
Owner: Codex
Branch: fix-as-const-excess-property-ts2353-20260512
Issue: #5835

## Scope

Restore TS2353 excess property checking when an object literal is assigned through an `as const` assertion to a narrower target type.

## Plan

- Add a focused regression for `const point: Point = { x, y, z } as const`.
- Trace the assignment/excess-property path for assertion expressions.
- Preserve normal type assertions (`as Point`) as an explicit bypass while treating const assertions as object-literal-originated for excess-property purposes.

## Checkpoint - 2026-05-12

Status: ready

Implemented const-assertion-aware excess property checking by unwrapping `expr as const` to the inner object literal for EPC while leaving ordinary type assertions opaque. Non-fresh const-asserted object types now contribute their explicit source keys for TS2353.

Validation:

- `cargo fmt --all`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo test -p tsz-checker --test ts2353_tests const_assertion_assignment_reports_excess_property -- --nocapture` -> 1 passed
