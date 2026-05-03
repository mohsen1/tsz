---
name: TS2345 widen literal source against non-primitive-union targets
status: claimed
timestamp: 2026-05-03 07:00:53
branch: fix/checker-ts2345-widen-literal-for-non-primitive-union
---

# Claim

Workstream 1 (Diagnostic Conformance) — fingerprint parity for TS2345 call argument display.

## Problem

`call_target_preserves_literal_argument_surface` returns `true` for any union
target, not just literal-sensitive ones. As a result, when the parameter type
is a union that mixes a non-primitive member with `null` / `undefined`
(e.g. `object | null`, `Object.create`'s declared signature in lib.es5), the
checker preserves the literal source text instead of widening it.

tsc only preserves literal source when the union is composed of primitive
members. For `object | null`, tsc widens the literal source (`1` → `number`,
`"string"` → `string`).

## Fix

Tighten `call_target_preserves_literal_argument_surface` to require that all
union members are primitive (per
`tsz_solver::visitor::is_primitive_type`). The existing
`is_literal_sensitive_assignment_target` gate at line 1211 of
`display_formatting.rs` already covers literal-sensitive targets; the broader
fallback is reserved for primitive-only unions like `boolean | null | undefined`
where tsc preserves the literal text.

## Tests

- New: `ts2345_call_argument_display_widens_literal_for_object_with_null_target`
- New: `ts2345_call_argument_display_widens_literal_for_object_target_param_name_independent`
  (locks the rule is structural, not tied to identifier names)
- Existing tests verified to still pass: `ts2345_call_parameter_display_preserves_semantic_nullable_union`,
  `ts2345_call_argument_display_widens_literal_for_non_union_target`,
  `ts2345_call_argument_display_widens_literal_for_optional_parameter_target`.

## Conformance impact

`objectCreate-errors.ts` source-side now matches tsc; target side (`object | null`
vs `object`) is a separate stripping rule not in scope here.
Net +3 conformance against current main.
