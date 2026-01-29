# Tuple Type Assignability Bug Investigation

## Issue Found
tsz does not emit errors when assigning arrays to incompatible tuples.

## Test Case
```typescript
const arr = [1, "hello"];  // Should infer as (string | number)[]
const tup: [number] = arr;  // Should ERROR but doesn't
```

**TypeScript Output**:
```
error TS2322: Type '(string | number)[]' is not assignable to type '[number]'.
  Target requires 1 element(s) but source may have fewer.
```

**tsz Output**: No errors (BUG!)

## Investigation

### Initial Approach
1. Added debug output to `check_subtype` to see what types are being compared
2. Expected to see: `Array(...) <: Tuple(...)` case being hit
3. Result: Unable to add debug output due to file editing issues

### Gemini's Analysis
Gemini suggested the root cause is likely one of:
1. **Array literals inferred as Tuples** instead of Arrays
   - If `[1, "hello"]` is inferred as `[number, string]` (a Tuple), then the checker takes the Tuple-to-Tuple path instead of Array-to-Tuple
   - This would bypass the array-to-tuple subtype check
   
2. **check_tuple_subtype bug** allowing arity mismatches
   - If source is `[number, string]` (length 2) and target is `[number]` (length 1)
   - The checker might only check up to the target's length and ignore extra elements

### Next Steps to Debug
1. Add debug output at the TOP of `check_subtype` to see what TypeKeys are actually being compared
2. Verify whether array literals are inferred as Arrays or Tuples
3. Check if the Tuple-to-Tuple case correctly handles length mismatches

## Current Status
🟡 **IN PROGRESS** - Investigation blocked by file editing issues. Need to:
1. Successfully add debug output to understand the type comparison
2. Identify which code path is being taken
3. Fix the root cause in the appropriate location

## Files to Investigate
- `src/solver/subtype.rs` - Main subtype checking dispatch logic (lines 633-647)
- `src/solver/subtype_rules/tuples.rs` - Tuple subtype checking logic (line 47+)
- `src/checker/type_computation.rs` - Array literal type inference

## Impact
This is a significant bug affecting the tuple category (7.4% pass rate, 25 failing tests).
