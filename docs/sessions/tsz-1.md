# Session tsz-1: Class Duplicate Detection Fix

**Started**: 2026-02-04 (Third iteration)
**Goal**: Fix duplicate getter/setter detection to emit correct number of errors

## Previous Session
- **Completed**: TS2318 core global type checking fix ✅
- **Commit**: `139809d7e`

## Current Task: Fix TS2300 Duplicate Getter Detection

### Problem Statement
Test `test_class_duplicate_getter_2300` expects 2 TS2300 errors for duplicate getters but only gets 1.

**Test Case**:
```typescript
class Rectangle {
    get width(): number {
        return 1;
    }

    get width(): number {
        return 2;
    }
}
```

**Expected**: 2 TS2300 errors (one for each duplicate getter)
**Actual**: 1 TS2300 error

### Investigation Needed
1. Find where duplicate identifier checking happens for class members
2. Understand why only one error is emitted instead of two
3. Fix to emit error for ALL duplicates, not just the first one

### Files to Check
- Class member duplicate detection in checker
- Symbol merging logic for class members
- Duplicate identifier emission code

### Success Criteria
- Test passes with exactly 2 TS2300 errors
- Similar duplicate cases (setters, methods) also fixed

## Status: NEXT TASK IDENTIFIED
Ready to begin investigation and implementation.
