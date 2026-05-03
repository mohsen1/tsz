---
name: TS2412 NUIA bail-out scoped to ELEMENT_ACCESS writes
status: claimed
timestamp: 2026-05-03 13:13:19
branch: fix/checker-ts2412-property-access-not-nuia
---

# Claim

Workstream 1 (Diagnostic Conformance) — TS2412 emission for writes to
optional named properties under `exactOptionalPropertyTypes: true` was
incorrectly silenced when `noUncheckedIndexedAccess` was also set.

## Problem

For `obj.optProp = undefined` where `obj.optProp?: T` and the program has
`exactOptionalPropertyTypes: true` and `noUncheckedIndexedAccess: true`,
tsc reports TS2412. Tsz fell back to TS2322 because
`has_exact_optional_write_target_mismatch` short-circuits the
NUIA-widening case by checking `target | undefined == read_target` for
the optional property's read type.

That short-circuit was meant to avoid TS2412 for index-signature lookups
that NUIA widened (`{ [key: string]: V }[k]` reads as `V | undefined`
even when the index entry isn't optional). But the same `target |
undefined == read_target` shape is the *normal* signature for a true `?`
optional named property. The bail-out fired for both, suppressing the
correct TS2412 on real optional named writes.

## Fix

Restrict the NUIA bail-out to `ELEMENT_ACCESS_EXPRESSION` writes (the
only shape NUIA widens). `PROPERTY_ACCESS_EXPRESSION` writes go through
the regular TS2412 path, so optional named property assignments under
`exactOptionalPropertyTypes` now match tsc.

## Tests

- All 3235 `tsz-checker` lib tests pass.
- Conformance net **+4** vs current main:
  `declarationEmitUsingAlternativeContainingModules1.ts`,
  `declarationEmitUsingAlternativeContainingModules2.ts`,
  `typeAssertionToGenericFunctionType.ts`, `valueOfTypedArray.ts`. Zero
  regressions.
