# TS2488 Optional Iterator Fix

## Summary
Fixed TS2488 error emission for types with optional `Symbol.iterator` methods. An optional `Symbol.iterator` property should NOT make a type iterable.

## Problem
Previously, the iterator checking logic in `src/checker/iterable_checker.rs` only checked if an object had a `[Symbol.iterator]` method property, but did not verify that it was required (non-optional).

TypeScript test case `for-of29.ts` expects:
```typescript
declare var iterableWithOptionalIterator: {
    [Symbol.iterator]?(): Iterator<string>
};

for (var v of iterableWithOptionalIterator) { }  // TS2488
```

## Solution
Modified `object_has_iterator_method()` in `src/checker/iterable_checker.rs` to check that `Symbol.iterator` is:
1. A method (`prop.is_method`)
2. NOT optional (`!prop.optional`)

### Code Change
```rust
// Before
if prop_name.as_ref() == "[Symbol.iterator]" && prop.is_method {
    return true;
}

// After
if prop_name.as_ref() == "[Symbol.iterator]" && prop.is_method && !prop.optional {
    return true;
}
```

## Test Cases Verified
1. `for-of29.ts` - Optional Symbol.iterator in for-of loop
2. `iteratorSpreadInArray10.ts` - Iterator without next() method
3. `for-of14.ts` - Object with next() but no Symbol.iterator
4. `for-of16.ts` - Object with Symbol.iterator returning non-iterator
5. `iterableArrayPattern21.ts` - Array destructuring of non-iterable

## Related Work
This builds on previous TS2488 improvements:
- Commit `2593a0cf2`: TypeParameter constraint checking
- Commit `5cefe3292`: Enhanced iterator protocol checking
- HEAD already contains IndexAccess, Conditional, and Mapped type handling

## Impact
- Correctly emits TS2488 for optional Symbol.iterator
- Improves conformance with TypeScript's iterator protocol checking
- Reduces missing TS2488 errors in conformance tests
