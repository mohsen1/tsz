# Session TSZ-2: Array Destructuring Type Narrowing

**Started**: 2026-02-05
**Status**: ✅ COMPLETE

## Goal

Fix type narrowing invalidation for array destructuring assignments.

## Summary

Successfully fixed all 3 failing control flow tests:
1. ✅ test_truthiness_false_branch_narrows_to_falsy (boolean narrowing bug)
2. ✅ test_array_destructuring_assignment_clears_narrowing
3. ✅ test_array_destructuring_default_initializer_clears_narrowing

## Root Cause Analysis

### Bug 1: Boolean Narrowing (Fixed Earlier)
**Problem**: Boolean type wasn't being narrowed to `false` in falsy branches.
**Root Cause**: `narrow_to_falsy` treated boolean same as string/number/bigint.
**Fix**: Added special case in `src/solver/narrowing.rs:2283-2296` to return `BOOLEAN_FALSE`.

### Bug 2: Array Destructuring Clearing (Fixed This Session)
**Problem**: `[x] = [1]` didn't clear narrowing on `x`.
**Root Cause**: `match_destructuring_rhs` tried to narrow to the RHS element type (literal 1), which is incorrect for destructuring.
**Fix**: Modified `src/checker/control_flow.rs:1344-1385` to return `None` for array patterns, triggering `initial_type` return to clear narrowing.

## Key Insight from Gemini Pro

"Array destructuring is fundamentally different from direct assignment:
- Direct assignment: `x = 1` narrows x to literal type 1
- Array destructuring: `[x] = [1]` CLEARS narrowing to declared type (string | number)

The reason: destructuring extracts a value from an array, whereas direct assignment assigns a specific value."

## Implementation Details

### Changed Files
1. `src/checker/control_flow.rs` - Modified `match_destructuring_rhs` to return `None` for array patterns
2. `src/checker/control_flow_narrowing.rs` - Removed debug tracing

### Critical Code Change
```rust
// src/checker/control_flow.rs:1344-1385
k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
    || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
{
    // CRITICAL FIX: For array destructuring, we should NOT narrow to the RHS element type.
    // Unlike direct assignment (x = 1 narrows x to literal 1), destructuring extracts
    // a value from an array, which clears the narrowing to the declared type.
    //
    // Example: [x] = [1] should clear x to string | number, not narrow to literal 1.
    //
    // By returning None here, we signal that the RHS type should not be used,
    // which causes get_assigned_type to return None, triggering initial_type return.
    return None;
}
```

## Test Results

All 3 tests now passing:
```
✅ test_truthiness_false_branch_narrows_to_falsy
✅ test_array_destructuring_assignment_clears_narrowing
✅ test_array_destructuring_default_initializer_clears_narrowing
```

## Commits

- `3d907e3a1`: fix(control_flow): array destructuring now clears type narrowing

## Notes

The investigation revealed that TypeScript's control flow analysis has subtle semantics:
1. Direct assignment preserves literal types for narrowing
2. Destructuring clears narrowing to declared type
3. This distinction is critical for matching tsc behavior exactly

## Next Steps

Session complete. All targeted control flow tests are passing.
