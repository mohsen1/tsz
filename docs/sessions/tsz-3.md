# Session tsz-3 - Discriminated Union Narrowing Fix

**Started**: 2026-02-04
**Status**: AWAITING INVESTIGATION
**Focus**: Fix discriminant narrowing to match TypeScript behavior

## Context

Previous session (tsz-3-discriminant-narrowing-investigation) revealed that discriminant narrowing is fundamentally broken. Gemini has recommended fixing the `narrow_by_discriminant` logic.

## Summary of Work Completed

### 1. Rewrote `narrow_by_discriminant` Function ✅

**Location**: `src/solver/narrowing.rs` lines ~232-328

**Changes**:
- Removed dependency on `find_discriminants` (too strict)
- Implemented filtering logic based on property value matching
- Uses `is_subtype_of` to check if property type matches literal value
- Excludes members without the property (x.prop === val implies prop exists)

**Verification**:
- Unit tests pass (`test_narrow_by_discriminant`, `test_narrow_by_discriminant_no_match`)
- All 62 narrowing tests pass
- Code compiles without errors

### 2. Discovered Integration Issue ⚠️

**Problem**: Discriminant narrowing is NOT triggered during end-to-end type checking of source files.

**Evidence**:
- Added extensive debug logging throughout the narrowing code paths
- Test case: `type D = { done: true, value: 1 } | { done: false, value: 2 }; if (o.done === true) { const y: 1 = o.value; }`
- `narrow_by_discriminant` is never called when checking actual source files
- `narrow_by_binary_expr` is never called
- Flow narrowing infrastructure exists but is not triggered for if statements

**What Works**:
- Unit tests pass (control flow unit tests including `test_switch_discriminant_narrowing`)
- Flow analysis infrastructure is in place
- `apply_flow_narrowing` is called for identifiers
- The narrowing logic itself is correct

**What Doesn't Work**:
- End-to-end type checking doesn't trigger discriminant narrowing for if statements
- The connection between flow analysis and type checking appears broken

## Current Status

The core `narrow_by_discriminant` rewrite is **complete and tested**. However, there's a deeper architectural issue: the flow narrowing is not being triggered during type checking of if statements in source files.

**This requires investigation into:**
1. How flow graph building integrates with type checking
2. Why discriminant_comparison is not called for if statement conditions
3. Whether flow analysis is enabled/built for regular source files
4. The connection between binder flow nodes and checker narrowing

## Test Cases

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
function test(o: D) {
    if (o.done === true) {
        const y: 1 = o.value; // Expected: no error, Actual: TS2322
    }
}
```

## Next Steps (For Future Session)

This session has completed the core rewrite but discovered an integration issue. The next session should:

1. **Investigate flow graph integration**: Understand why flow narrowing isn't triggered for if statements
2. **Check flow analysis initialization**: Verify that flow graphs are built for all source files
3. **Debug discriminant_comparison path**: Add tracing to understand why discriminant_comparison returns None or isn't called
4. **Consider alternative approaches**: Maybe if statements need different handling than switch statements
5. **Test with conformance suite**: Once working, run TypeScript conformance tests

## Commits

- `39f3736af`: feat: rewrite narrow_by_discriminant to filter union members
- `458d8e4cb`: debug: add extensive logging to discriminant narrowing code paths
- `2b1732ae2`: chore: remove debug logging from narrowing code
- `fb7389837`: Merge remote tracking branch 'origin/main'