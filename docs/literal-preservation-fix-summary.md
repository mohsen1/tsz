# Literal Type Preservation Fix - Session Summary

**Date**: 2026-02-13
**Status**: ✅ Fixed and Committed
**Commit**: d1d5f2371

## Problem

Discriminated union types were failing to narrow properly because literal types like `false` and `true` were being widened to `boolean` during contextual typing.

### Example

```typescript
type Result = { success: false } | { success: true };

const test: Result = {
    success: false  // ❌ Error: Type '{ success: boolean }' not assignable
};
```

## Root Cause

When extracting property types from discriminated unions, the `contextual_property_type` function would:

1. Extract property types from each union member: `[false, true]`
2. Create a union: `interner.union([false, true])`
3. The `union()` function calls `normalize_union()`
4. `normalize_union()` calls `absorb_literals_into_primitives()`
5. This simplifies `true | false` → `boolean` ❌

While this optimization is correct for general type theory, it breaks discriminated union narrowing where literal preservation is critical.

## The Fix

**File**: `crates/tsz-solver/src/contextual.rs:972`

**Change**: Use `union_preserve_members()` instead of `union()`

```rust
// BEFORE (broken)
Some(self.interner.union(prop_types))

// AFTER (fixed)
Some(self.interner.union_preserve_members(prop_types))
```

The `union_preserve_members()` function skips the `absorb_literals_into_primitives()` optimization, keeping literal types intact for contextual typing.

## Impact

### ✅ What Works Now

1. **Object literal contextual typing**: Literals preserve their types
   ```typescript
   const test: Result = { success: false };  // ✓ No error!
   ```

2. **Function return types**: Object literals in return statements work
   ```typescript
   function f(): Result {
       return { success: false };  // ✓ No error!
   }
   ```

3. **All unit tests pass**: 2394/2394 tests passing, no regressions

### 🔄 What Still Needs Work

The conformance tests show this fix alone doesn't improve the control flow pass rate (still 47/92, 51.1%) because:

1. **Narrowing logic for `let` vs `const` destructuring** is still missing
   - `const { data, isSuccess } = result;` should allow narrowing
   - `let { data, isSuccess } = result;` should NOT allow narrowing (mutable)

2. **Test status changed from "wrong error" to "missing error"**
   - Before: TS2322 (Type mismatch) - incorrect
   - After: No errors - but TSC expects TS1360/TS18048 for `let` cases
   - This is progress! The contextual typing is fixed, now need narrowing logic

## Next Steps

To fully fix discriminated union narrowing:

1. ✅ **Fix literal preservation** (this commit)
2. ⏭️ **Implement `let` vs `const` narrowing** (Task #3)
   - Track whether destructured bindings are mutable
   - Only allow narrowing for `const` bindings
   - Emit TS1360/TS18048 for `let` bindings that can't be narrowed

3. ⏭️ **Implement assertion function narrowing** (Task #2)
   - Already works for simple cases
   - May need refinement for discriminant narrowing

## Technical Details

### Type Interner Union Normalization

The TypeInterner has two union creation functions:

1. **`union(members)`** - Normalizes and optimizes
   - Flattens nested unions
   - Removes `never`
   - **Absorbs literals into primitives** (the issue!)
   - Reduces subtypes
   - Best for general type computation

2. **`union_preserve_members(members)`** - Preserves structure
   - Flattens nested unions
   - Removes `never`
   - **Keeps literals intact** ✓
   - No subtype reduction
   - Best for contextual typing and discriminated unions

### Why This Matters

Discriminated unions rely on literal types for narrowing:
```typescript
type Action =
    | { type: 'INCREMENT'; payload: number }
    | { type: 'DECREMENT'; payload: number };

// Property 'type' should be 'INCREMENT' | 'DECREMENT', NOT 'string'
```

If we widen to `string`, the discriminant loses its narrowing power.

## Files Changed

- `crates/tsz-solver/src/contextual.rs` - Use `union_preserve_members()`

## Testing

- ✅ All 2394 unit tests pass
- ✅ Manual test cases pass
- ⏭️ Conformance tests: awaiting narrowing logic implementation

## References

- **Analysis Document**: `docs/control-flow-narrowing-bug-analysis.md`
- **Test Case**: `tmp/test_contextual_literal.ts` (not committed, in tmp/)
- **Related Issue**: controlFlowAliasedDiscriminants.ts conformance test
