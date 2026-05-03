---
name: Equality narrowing of unknown against const intrinsic annotation
status: claimed
timestamp: 2026-05-03 10:15:07
branch: fix/checker-narrow-unknown-equality-against-const-intrinsic
---

# Claim

Workstream 1 (Diagnostic Conformance) — `if (u === aString)` where
`u: unknown` and `declare const aString: string` should narrow `u` to
`string` so the body can assign `u` to a `string` target without TS2322.

## Problem

Two gates were too strict:

1. `is_narrowing_literal` (in `tsz_solver::type_queries::flow`) only
   accepted unit types (literals + enum members + null/undefined). Primitive
   intrinsics like `TypeId::STRING` weren't recognized as valid comparands,
   so equality with a `string`-typed const never produced a
   `TypeGuard::LiteralEquality`.
2. `resolve_const_identifier_type` (in
   `flow/control_flow/narrowing.rs`) only resolved a const variable's
   annotation when the annotation was `null` or `undefined`. Primitive
   keyword annotations (`string`, `number`, …) and primitive type
   references (`type_arguments` empty) were rejected, so the narrowing
   path couldn't recover the comparand's shape during flow analysis.

## Fix

Two-part change:

- Solver: `is_narrowing_literal` now also returns `Some(type_id)` for
  primitive intrinsics (`TypeId::STRING`, `NUMBER`, `BOOLEAN`, `BIGINT`,
  `SYMBOL`, `OBJECT`) and `TypeData::UniqueSymbol`. `narrow_to_type` already
  has a special case for `unknown` / `any` sources that returns the target
  directly, so narrowing reaches the right shape without further changes
  to the equality guard pipeline.
- Checker: extracted `const_annotation_intrinsic_type` to a new
  `flow/control_flow/narrowing_helpers.rs` module (keeps `narrowing.rs`
  under the 2000-LOC architecture ceiling). The helper resolves both
  primitive keyword annotations and no-arg type-reference annotations
  whose name is a primitive identifier.

The structural rule, in one sentence: *"When `u: unknown` is compared
against a value whose declared type is a primitive intrinsic, tsc
narrows `u` to that intrinsic; this change makes tsz do the same."*

## Tests

- New: `tests/equality_narrow_unknown_to_const_intrinsic_tests.rs` with
  four cases — `string`, `number`, `boolean`, and a name-independence
  check — exercising the const-annotation path. 4/4 passing.
- Existing 3216 `tsz-checker` lib tests pass.

## Conformance impact

Net **+1** vs current main: `flatArrayNoExcessiveStackDepth.ts` flips
to PASS. `unknownType2.ts` remains a fingerprint-only failure for the
remaining cases (`unique symbol`, enum, `object` against object-literal
RHS) but the simple primitive-typed-const cases (lines 65, 69, 73, 77)
now match tsc's expected behaviour. Zero regressions.
