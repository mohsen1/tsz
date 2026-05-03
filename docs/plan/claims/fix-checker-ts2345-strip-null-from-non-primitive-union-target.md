---
name: TS2345 strip null/undefined from non-primitive union target
status: claimed
timestamp: 2026-05-03 07:37:55
branch: fix/checker-ts2345-strip-null-from-non-primitive-union-target
---

# Claim

Workstream 1 (Diagnostic Conformance) — fingerprint parity for TS2345 call parameter display.

## Problem

For non-optional union parameters mixing a non-primitive member with
`null` / `undefined` (e.g. `object | null` from `Object.create`), tsc
strips the nullish members in the TS2345 target display:

  Argument of type 'number' is not assignable to parameter of type **'object'**.

Tsz preserved the full union:

  Argument of type 'number' is not assignable to parameter of type 'object | null'.

But tsc DOES preserve the union when stripping leaves only primitive
members (e.g. `boolean | null | undefined` → preserve full union — locked
by `ts2345_call_parameter_display_preserves_semantic_nullable_union`).

## Fix

Add a `strip_nullish_for_non_primitive_union_target` helper that:
1. Calls the existing `strip_nullish_for_assignability_display` helper to
   compute the strip candidate.
2. Returns the stripped type only when the result contains at least one
   non-primitive member (per `tsz_solver::visitor::is_primitive_type`).
3. Otherwise returns `None`, falling through to the existing
   `format_assignability_type_for_message_preserving_nullish` path which
   keeps the full union.

Wired into `format_call_parameter_type_for_diagnostic` after the
optional-parameter strip path.

## Tests

- New: `ts2345_call_parameter_display_strips_null_for_non_primitive_union`
- New: `ts2345_call_parameter_display_strips_null_param_name_independent`
  (locks the rule is structural per anti-hardcoding directive)
- Existing tests verified: `ts2345_call_parameter_display_preserves_semantic_nullable_union`
  (boolean | null | undefined preserved), and 7 other TS2345 call tests.

## Conformance impact

Net +4 against current main. `objectCreate-errors.ts` flips to PASS,
plus 3 incidental wins (`strictOptionalProperties3.ts`,
`tsxAttributeResolution6.tsx`, `typeFromParamTagForFunction.ts`). Zero
regressions.
