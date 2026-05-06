# Claim: Fix JSX intrinsic type-argument diagnostics

## Target

`TypeScript/tests/cases/compiler/jsxIntrinsicElementsTypeArgumentErrors.tsx`

Current filtered conformance on `origin/main`: 0/1 passed.

Expected diagnostics include TS1009, TS2304, TS2344, and TS2558 for JSX intrinsic elements with invalid type arguments.

Actual diagnostics currently miss those fingerprints.

## Plan

Investigate JSX opening/self-closing element type-argument parsing, lowering, and validation. Ensure intrinsic JSX elements with type arguments report the same parser/name/constraint/count diagnostics as tsc without changing ordinary JSX elements.

## Verification

Planned:

- focused parser/checker regression
- filtered conformance for `jsxIntrinsicElementsTypeArgumentErrors`
