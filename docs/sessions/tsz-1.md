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

## Progress

### 2026-02-04: Fixed Duplicate Getter Detection ✅

**Root Cause**:
In `src/checker/state_checking_members.rs:411`, the code used `.skip(1)` when iterating through duplicate accessors, which only emitted TS2300 for subsequent duplicates, missing the first one.

**Fix Applied**:
Changed from:
```rust
for &idx in indices.iter().skip(1) {
```
To:
```rust
for &idx in indices.iter() {
```

**Testing**:
- ✅ Test case: `test_class_duplicate_getter_2300` now passes
- ✅ Verified with manual test file - emits 2 TS2300 errors (matching tsc)
- ✅ tsc emits errors for BOTH duplicate declarations

**Example**:
```typescript
class Rectangle {
    get width(): number { return 1; }  // Now emits TS2300 ✓
    get width(): number { return 2; }  // Already emitted TS2300 ✓
}
```

**Commit**: `bdbe02b78` - "fix: emit TS2300 for all duplicate accessors, not just subsequent ones"

## Session Status: COMPLETE ✅

**Deliverable**: Fixed duplicate getter/setter detection to match tsc behavior
**Result**: TS2300 now emitted for ALL duplicate accessor declarations
