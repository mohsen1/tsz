---
name: Fix checker ignore variance when TS2637 emitted
description: When a type alias body is not one of the variance-supported forms (object / function / constructor / mapped), suppress the user's `in`/`out` annotation in the relation checker so assignability falls back to the computed (default) variance, matching tsc.
type: project
branch: fix-checker-ignore-variance-when-ts2637-emitted
status: ready
scope: checker (TS2637 / variance enforcement)

## Summary

`varianceReferences.ts` expected `vcn12 = vcn1` (where `vcn1:
VarianceConstrainedNumber<1>` and `vcn12: VarianceConstrainedNumber<1
| 2>`) to be assignable. tsc allows it because the alias body is not a
supported form for variance annotations — TS2637 fires and the user's
`in out` annotation is **ignored** for assignability, leaving the
computed (default-covariant) variance in effect.

tsz still enforced the user-declared invariance, producing 5 false
positives (`Type '1' is not assignable to type
'VarianceConstrainedNumber<1 | 2>'`, etc.).

## Fix

`declared_type_param_variances_for_node` (resolver.rs) for type aliases
now checks the body kind against the same set used by the TS2637
check (`TYPE_LITERAL` / `FUNCTION_TYPE` / `CONSTRUCTOR_TYPE` /
`MAPPED_TYPE`). When the body is not a supported form, return `None` so
the variance computation falls through to the standard (computed)
default — matching tsc's runtime behavior of ignoring rejected
annotations.

## Files Changed

- `crates/tsz-checker/src/context/resolver.rs`

## Verification

- Conformance: net **+3** (12304 → 12307). 3 improvements, 0 regressions.
  - `varianceReferences.ts` (target) flips fingerprint-only → PASS
  - `optionalParameterInDestructuringWithInitializer.ts`,
    `intersectionThisTypes.ts` — incidental flips.
- Unit tests: tsz-checker (3103) + tsz-solver (5576) all green.
