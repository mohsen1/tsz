# Emit Testing Session - Temp Variable Scope Fix
**Date:** 2026-02-12
**Focus:** Slice 4 - Phase 1 Implementation

## Summary

Implemented Phase 1 from the emit slice 4 status document: fixed temp variable naming in function scopes. Functions now correctly reset temp variable counters, allowing inner functions to reuse names like `_i` and `_a`.

## Problem

Temp variable names (like `_i`, `_a`, `_b`, `_c`) were incrementing globally across function boundaries. Nested functions would continue the counter instead of resetting it.

**Example (ES5For-of19):**
```typescript
for (let v of []) {
    function foo() {
        for (let v of []) {}
    }
}
```

**Before (incorrect):**
```javascript
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    function foo() {
        for (var _b = 0, _c = []; _b < _c.length; _b++) {  // Wrong!
            // ...
        }
    }
}
```

**After (correct):**
```javascript
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    function foo() {
        for (var _i = 0, _a = []; _i < _a.length; _i++) {  // Correct!
            // ...
        }
    }
}
```

## Root Cause

The temp variable naming infrastructure existed (`push_temp_scope()` and `pop_temp_scope()` methods) but wasn't being used for regular function declarations and expressions.

**Existing usage:**
- ✅ ES5 parameter transforms already used scope management
- ❌ Regular function declarations didn't push/pop scope
- ❌ Regular function expressions didn't push/pop scope

## Solution

Added `push_temp_scope()` and `pop_temp_scope()` calls around function body emission in:

1. **Function Declarations** (`crates/tsz-emitter/src/emitter/declarations.rs:65-67`)
2. **Function Expressions** (`crates/tsz-emitter/src/emitter/functions.rs:245-248`)

```rust
// Push temp scope for function body - each function gets fresh temp variables
self.push_temp_scope();
self.emit(func.body);
self.pop_temp_scope();
```

This ensures:
- Each function starts with temp counter at 0
- First for-of gets special `_i` index name
- Temp names reset to `_a`, `_b`, `_c` sequence
- Generated names tracked separately per function scope

## Test Results

### ES5For-of Tests
- **Before:** 70% pass rate (14/20)
- **After:** 70% pass rate (14/20)
- **Note:** Pass rate same, but diffs improved (temp naming fixed, only shadowing remains)

### Overall Sample (200 tests)
- **Before:** 68.2% (120/176)
- **After:** 69.9% (123/176)
- **Improvement:** +3 tests passing

### Specific Test Improvements

**ES5For-of19 diff improvement:**
```diff
# Before fix
-        for (var _b = 0, _c = []; _b < _c.length; _b++) {
+        for (var _i = 0, _a = []; _i < _a.length; _i++) {

# After fix
+        for (var _i = 0, _a = []; _i < _a.length; _i++) {
```

Tests now only differ on variable shadowing (`v` vs `v_1`), not temp naming.

## What's Fixed

✅ **Temp variable naming in nested functions**
- Inner functions now reuse `_i`, `_a` names
- Counter resets to 0 for each function
- `first_for_of_emitted` flag resets per function

## What's NOT Fixed (Remaining Issues)

❌ **Variable Shadowing** (Phase 3)
- Inner `v` should be `v_1` when shadowing outer `v`
- ES5For-of15, ES5For-of16, ES5For-of17 still fail on this
- Requires scope tracking to detect name conflicts

❌ **This Capture Pattern** (Phase 2)
- Still using IIFE pattern instead of `var _this = this;`
- Arrow function tests still at 25% pass rate
- Separate issue from temp variable naming

## Code Changes

**Files Modified:**
- `crates/tsz-emitter/src/emitter/declarations.rs` - Added scope mgmt to function declarations
- `crates/tsz-emitter/src/emitter/functions.rs` - Added scope mgmt to function expressions

**Lines Changed:** 8 (4 insertions in each file)

## Quality Checks

✅ **Unit Tests:** All 2,396 passing, 40 skipped
✅ **Pre-commit:** All checks passed (formatting, clippy, tests)
✅ **No Regressions:** Verified with emit test sample

## Impact Analysis

**Direct Impact:**
- 3 additional tests passing in 200-test sample
- Improved diffs in ~10 tests (smaller remaining differences)
- Foundation for variable shadowing fixes

**Indirect Benefits:**
- Cleaner temp variable output matches TSC better
- Easier to debug emitted code
- Reduces noise in test diffs (focuses on real issues)

## Next Steps

Following the phased approach from `docs/emit-slice4-status.md`:

**✅ Phase 1: Temp Variable Naming** - COMPLETED
- Fixed temp counter reset in function scopes
- Tests improved as expected

**→ Phase 2: This Capture Pattern** - NEXT
- Change arrow function emission from IIFE to `var _this = this;`
- High impact: estimated 20-40 tests
- Medium complexity

**→ Phase 3: Variable Shadowing** - LATER
- Track declared variables in current function scope
- Add `_1`, `_2` suffixes for shadowed names
- Moderate impact: estimated 5-10 tests
- High complexity (needs scope tracking)

## Commit

- `3e0200c2e` - fix: reset temp variable naming in function scopes

---

**Session Duration:** ~1.5 hours
**Tests Improved:** +3 direct, ~10 cleaner diffs
**Foundation Laid:** Ready for Phase 2 (this capture) implementation
