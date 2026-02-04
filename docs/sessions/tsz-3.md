# Session tsz-3 - Discriminated Union Narrowing Fix

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Fix discriminant narrowing to match TypeScript behavior

## Context

Previous session (tsz-3-discriminant-narrowing-investigation) revealed that discriminant narrowing is fundamentally broken. Gemini has recommended fixing the `narrow_by_discriminant` logic.

## Current Task: Fix Discriminated Union Narrowing Logic

### Problem Statement

Current implementation in `src/solver/narrowing.rs` uses `find_discriminants` which enforces strict rules that don't match TypeScript's behavior. TypeScript treats discriminant narrowing as a filtering operation, not strict structure validation.

### Current Implementation (WRONG)

The `find_discriminants` function (used by `narrow_by_discriminant`) enforces:
- Property must exist on **all** union members
- Values must be **unique** across members
- This is too strict and doesn't match tsc behavior

### Solution (from Gemini)

**Rewrite `narrow_by_discriminant`** to:
1. Stop using `find_discriminants`
2. Iterate through union members of `union_type`
3. For each member, resolve the type of `property_name`
4. If property type matches `literal_value` (or is a subtype), include the member
5. If member doesn't have the property, exclude it (since `x.prop === val` implies prop exists)
6. Return union of matching members

**Location**: `src/solver/narrowing.rs` lines ~268-297 (narrow_by_discriminant function)

### Test Cases

```typescript
// Case 1: Shared discriminant values
type A = { kind: "group1", value: number };
type B = { kind: "group1", name: string };
type C = { kind: "group2", active: boolean };
type U1 = A | B | C;

function f1(x: U1) {
    if (x.kind === "group1") {
        // Should narrow to A | B
    }
}

// Case 2: Mixed with null
type U2 = { type: "ok", data: string } | { type: "error", code: number } | null;

function f2(x: U2) {
    if (x && x.type === "ok") {
        // Should narrow to { type: "ok", data: string }
    }
}
```

### Implementation Plan

1. Read current `narrow_by_discriminant` implementation
2. Rewrite to filter union members based on property value matching
3. Test with simple cases
4. Run conformance tests to verify improvement