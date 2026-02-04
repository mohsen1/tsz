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

**Location**: `src/solver/narrowing.rs` lines ~232-328 (narrow_by_discriminant function)

## Progress

### Completed Work

1. **Rewrote `narrow_by_discriminant`** (commit pending):
   - Removed dependency on `find_discriminants`
   - Implemented filtering logic based on property value matching using `is_subtype_of`
   - Added import for `is_subtype_of` from `subtype` module
   - Code compiles successfully

2. **Investigation Findings**:
   - Control flow unit tests pass (including `test_switch_discriminant_narrowing`)
   - Flow narrowing infrastructure is in place (`apply_flow_narrowing` is called)
   - The `narrow_by_discriminant` function is NOT being called in actual type checking
   - Problem is earlier in the flow analysis - discriminant narrowing is not triggered for if statements

### Current Issue

**Root Cause**: The discriminant narrowing is NOT being triggered for if statement conditions.

When testing with:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
let o: D;
if (o.done === true) {
    const y: 1 = o.value; // Should work but gets TS2322 error
}
```

**Findings**:
- `narrow_by_discriminant` is never called (added debug logging to confirm)
- The flow analysis doesn't recognize `o.done === true` as a discriminant comparison
- Switch statement discriminant narrowing works (unit test passes)
- If statement discriminant narrowing doesn't work (integration test fails)

### Next Steps

Need to investigate why discriminant narrowing is not triggered for if statements:
1. Check how flow graph is built for if statement conditions
2. Verify `discriminant_comparison` is being called for binary expressions
3. Ensure the flow node for the if statement is correctly set up
4. Check if the problem is in `narrow_by_binary_expr` or earlier in the flow

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

// Case 3: Simple discriminant (current test case)
type D = { done: true, value: 1 } | { done: false, value: 2 };
let o: D;
if (o.done === true) {
    const y: 1 = o.value; // Expected: no error, Actual: TS2322
}
```

### Implementation Plan

1. ✅ Rewrite `narrow_by_discriminant` to filter union members based on property value matching
2. ⏳ Investigate why discriminant narrowing is not triggered for if statements
3. ⏳ Debug flow graph construction for if statement conditions
4. ⏳ Test with simple cases
5. ⏳ Run conformance tests to verify improvement