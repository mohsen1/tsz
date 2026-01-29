# Intersection Type Assignability Bug

## Issue Found
tsz does not emit errors when assigning types to incompatible intersections.

## Test Case
```typescript
type A = { x: number };
type B = { x: string };
type AB = A & B;

// Should error - A is not assignable to AB
const val: AB = { x: 1 } as A;
```

**TypeScript Output**:
```
error TS2322: Type 'A' is not assignable to type 'AB'.
  Type 'A' is not assignable to type 'B'.
    Types of property 'x' are incompatible.
      Type 'number' is not assignable to type 'string'.
```

**tsz Output**: No errors (BUG!)

## Root Cause Investigation

### Initial Hypothesis (INCORRECT)
Originally thought the issue was that type aliases `A` and `B` might not be in the type environment when the assignability check runs.

### Attempted Fix 1: Eager Resolution (FAILED)
Tried to resolve dependencies during type alias computation in `compute_type_of_symbol`. This broke the symbol cache because:

1. Re-entrancy problem: calling `get_type_of_symbol` recursively while computing a type
2. ERROR placeholder pollution: the pre-cached ERROR placeholder was being used during recursive resolution
3. Cache corruption: multiple different symbols ended up mapped to the same TypeId

### Correct Approach: Lazy Resolution at Check Site (IMPLEMENTED)
Following Gemini's recommendation, implemented lazy resolution at the check-site rather than eager resolution at definition time.

**Added `ensure_refs_resolved` method** in `src/checker/assignability_checker.rs`:
- Recursively walks a type tree
- For each `Ref(symbol)` encountered, calls `get_type_of_symbol(symbol)` to ensure it's resolved and in type_env
- Called from both `is_assignable_to` and `is_subtype_of` before the actual check
- Also handles intersections, unions, applications, functions, objects, etc.

### Current Status
The lazy resolution infrastructure is now in place, but the bug is NOT yet fixed. The issue appears to be deeper:

**Debug Output Shows**:
```bash
DEBUG: is_assignable_to source=TypeId(105) target=TypeId(105)
```

For `const val1: AB = { x: 1 }`, both source and target are TypeId(105) - the same type! This means:
1. The type annotation `AB` is being lowered to TypeId(105)
2. The initializer `{ x: 1 }` is also being inferred as TypeId(105)
3. So we're checking TypeId(105) against TypeId(105), which always passes

### Hypothesis: Type Aliases Are Fully Expanded During Lowering
The real issue might be that when we lower `type AB = A & B`, the type lowering is:
1. Immediately expanding `A` to its structural type `{ x: number }`
2. Immediately expanding `B` to its structural type `{ x: string }`
3. Creating `Intersection([{x: number}, {x: string}])`
4. TypeScript infers this as `{x: never}`
5. But the assignability check never happens because both sides resolve to the same type

### Next Steps
1. Investigate type lowering to verify if type aliases are being fully expanded
2. Check if there's a structural normalization that's making incompatible types appear compatible
3. Look at how TypeScript handles this - does it keep type aliases as references during checking?

## Impact
This is a significant type checking gap that affects conformance, particularly for:
- Complex type compositions
- Generic type constraints
- Union/intersection interactions

## Status
🟡 **IN PROGRESS** - Lazy resolution infrastructure implemented, but bug not fixed yet. Need to investigate type lowering and structural normalization.
