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

### Subtype Checker Logic
Location: `src/solver/subtype.rs:538-547`

```rust
(_, TypeKey::Intersection(members)) => {
    // Check if source is a subtype of ALL intersection members
    let member_list = self.interner.type_list(*members);
    for &member in member_list.iter() {
        if !self.check_subtype(source, member).is_true() {
            return SubtypeResult::False;
        }
    }
    Subtype::True
}
```

This logic should work correctly:
- Check if `A` is assignable to `A` (should be True)
- Check if `A` is assignable to `B` (should be False)
- Result: False

### Hypothesis
The type aliases `A` and `B` might be resolved to their underlying types before the assignability check, causing the check to be bypassed.

## Next Steps
1. Verify type alias resolution flow
2. Check if intersection types are being normalized
3. Add debug logging to trace the check
4. Fix the root cause

## Impact
This is a significant type checking gap that affects conformance, particularly for:
- Complex type compositions
- Generic type constraints
- Union/intersection interactions

## Status
🔴 **IN PROGRESS** - Root cause investigation needed
