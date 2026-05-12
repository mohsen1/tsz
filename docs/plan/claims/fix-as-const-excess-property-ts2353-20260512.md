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
