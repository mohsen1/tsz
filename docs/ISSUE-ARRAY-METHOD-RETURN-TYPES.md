# Issue: Array Method Return Types Not Simplified

## Problem

Array methods like `.sort()`, `.map()`, etc. are returning the full array interface structure instead of a simplified array type.

## Example

```typescript
interface Item {
    name?: string;
}

class Container {
    items: Item[];

    sortItems() {
        this.items = this.items.sort((a, b) => {
            return a.name! < b.name! ? -1 : 1;
        });
        // Error: Type '{ (index: number, value: T): T[]; ... }' is not assignable to type 'Item[]'
    }
}
```

### Expected Behavior (TSC)
No error - `.sort()` should return `Item[]`

### Actual Behavior (tsz)
Error showing the entire array interface expanded with all methods

## Root Cause

When resolving the return type of array methods, we're not simplifying/normalizing the type back to its array form. The type system correctly identifies that `.sort()` returns an array type, but doesn't collapse the expanded interface representation back to `T[]`.

## Impact

- **False Positive Tests**: 23 tests in conformance suite 0-500
- **Specific Tests Affected**:
  - `arrayconcat.ts`
  - Other array method tests

## Files to Investigate

1. **Property Access Resolution**: `crates/tsz-solver/src/type_queries.rs`
   - How we resolve property access on array types
   - Should simplify array interface to array type

2. **Type Normalization**: Type simplification after property access
   - Need to detect when a type is semantically an array
   - Collapse to `T[]` form

3. **Method Return Type Resolution**:
   - When `.sort()` is called on `T[]`, it returns `T[]`
   - Currently returning the full interface type instead

## Proposed Fix

Add a normalization step when resolving property access on array types:

1. After resolving a property on an array, check if the result type is semantically an array
2. If it's an interface/object type that represents an array (has all array methods), simplify it to `T[]`
3. Extract the element type from the interface and create a simplified array type

## Test Cases

**Minimal repro**: `tmp/test-array-method-return.ts`
**Conformance test**: `TypeScript/tests/cases/compiler/arrayconcat.ts`

## Priority

**Medium-High** - Affects multiple tests and creates confusing error messages with massive type expansions in errors.
