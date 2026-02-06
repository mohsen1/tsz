# Session TSZ-3-Flow: In Operator Narrowing Bug Fix

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Focus**: Fix in operator narrowing for union types with optional properties

## Problem Statement

**Immediate Issue**: The test `test_in_operator_optional_property_keeps_false_branch_union` was failing because in operator narrowing for union types was incorrectly creating intersection types and including NEVER values in the result.

## Root Cause Investigation

### Initial Hypothesis (INCORRECT)
The session initially thought this was a "flow node association issue" where else branch expressions were getting the wrong flow node.

### Actual Root Cause (CORRECT)
The issue was in `src/solver/narrowing.rs` in the `narrow_by_property_presence` function:

1. **NEVER type pollution**: When narrowing a union type with `"prop" in x`, union members without the property became NEVER, but these NEVER types were INCLUDED in the resulting union, causing incorrect narrowing.

2. **Unnecessary intersection**: For union narrowing, the code was creating intersections like `type & { prop: type }` instead of just filtering the union members.

## Solution

### Changes Made

**File**: `src/solver/narrowing.rs` - `narrow_by_property_presence` function

1. **Filter NEVER types before union creation**:
   ```rust
   let matching_non_never: Vec<TypeId> = matching
       .into_iter()
       .filter(|&t| t != TypeId::NEVER)
       .collect();
   ```

2. **Keep union members as-is instead of intersecting**:
   ```rust
   if present {
       if has_property {
           // Property exists: Keep the member as-is
           // CRITICAL: For union narrowing, we don't modify the member type
           member  // Instead of: self.db.intersection2(member, filter_obj)
       }
   }
   ```

### Example Fix
```typescript
let x: { a?: number } | { b: string };
if ("a" in x) {
  x; // Before: union of [NEVER, { a?: number } & { a: number }]  (WRONG)
     // After:  { a?: number }                                   (CORRECT)
} else {
  x; // Before: union of [NEVER, { b: string }]                  (WRONG)
     // After:  { a?: number } | { b: string }                   (CORRECT)
}
```

## Success Criteria

- [x] Identify root cause of flow node association issue
- [x] Fix narrowing logic for union types with in operator
- [x] `test_in_operator_optional_property_keeps_false_branch_union` passes
- [x] All 10 in operator narrowing tests passing
- [x] Gemini Pro review: **APPROVED**
- [x] Commit and push fixes

## Test Results

- All 10 in operator tests pass
- All 82 control flow tests pass
- No regressions introduced (8 pre-existing test failures remain)

## Files Modified

- `src/solver/narrowing.rs` - Fixed union narrowing in `narrow_by_property_presence`
- `src/checker/tests/control_flow_tests.rs` - Removed `#[ignore]` from passing test
- `src/binder/state.rs` - Added trace logging for if statement binding (useful for future debugging)
- `src/binder/state_binding.rs` - Added trace logging for record_flow (useful for future debugging)

## Key Learnings

1. **Session diagnosis was wrong**: The initial hypothesis about "flow node association" was incorrect. The flow nodes were actually correct - the issue was in the narrowing logic itself.

2. **Debug tracing is essential**: Adding eprintln! debugging helped quickly identify the actual bug (NEVER types being included in union).

3. **Gemini consultation is valuable**: Gemini Pro review confirmed the fix was correct and identified edge cases.

---

*Session completed by tsz-3 on 2026-02-06*
