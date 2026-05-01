---
name: Fix checker TS2367 widen boolean true/false
description: Recognize `BOOLEAN_TRUE`/`BOOLEAN_FALSE` intrinsics as the boolean family in `get_primitive_family`, so cross-family TS2367 widening matches tsc (`'true'` → `'boolean'` in messages).
type: project
branch: fix-checker-ts2367-widen-boolean-true-false
status: ready
scope: checker (TS2367 / cross-family widening)

## Summary

`get_primitive_family` (in binary.rs) returned `ERROR` for `BOOLEAN_TRUE`
/ `BOOLEAN_FALSE` because both are intrinsic `TypeId`s and
`classify_literal_type` short-circuits on intrinsics. That defeated the
cross-family widening in TS2367 messages — `s == true` rendered as
`'symbol' and 'true'` instead of tsc's `'symbol' and 'boolean'`.

## Fix

Add an explicit fast-path for boolean-literal intrinsics in
`get_primitive_family`: when `type_id == BOOLEAN_TRUE` or
`BOOLEAN_FALSE`, return `BOOLEAN`. Subsequent
`widen_literal_type(true)` already returns `boolean`, so the message
text now matches tsc.

## Files Changed

- `crates/tsz-checker/src/types/computation/binary.rs`

## Verification

- Conformance: net **+5** (12304 → 12309). 5 improvements, 0 regressions.
  - `symbolType9.ts` flips fingerprint-only → PASS (the targeted fix).
  - 4 incidental flips (carryover from other landed fixes).
- Unit tests: tsz-checker (3103) all green.
