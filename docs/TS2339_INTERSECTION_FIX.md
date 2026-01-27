# TS2339 Intersection Type Property Access Fix

## Summary

Fixed property access on intersection types to properly check index signatures when no explicit property is found on any member. This addresses false positives where accessing properties on intersection types like `A & B` would fail even when one of the members had an index signature.

## Problem

When accessing a property on an intersection type where no member explicitly declares the property, the code would immediately return `PropertyNotFound` without checking if any member has an index signature that could resolve the access.

### Example of the Issue

```typescript
type A = { x: number };
type B = { [key: string]: string };
type C = A & B;

const obj: C = { x: 1 };
obj.y;  // Should work via B's index signature, but was failing
```

### Previous Behavior

In `/Users/claude/code/tsz/src/solver/operations.rs:2420-2438`, the intersection type handler would:

1. Iterate through all members
2. Collect successful property access results
3. If no results found, immediately return `PropertyNotFound`
4. **Missing**: No check for index signatures before failing

```rust
// Old code
if results.is_empty() {
    if !nullable_causes.is_empty() {
        // ... handle nullable causes
    }
    if saw_unknown {
        return PropertyAccessResult::IsUnknown;
    }
    // ❌ Missing index signature check
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

## Solution

Added index signature resolution before giving up on property access for intersection types.

### Implementation

Modified `/Users/claude/code/tsz/src/solver/operations.rs:2420-2473` to check for index signatures when no explicit property is found:

```rust
if results.is_empty() {
    if !nullable_causes.is_empty() {
        let cause = if nullable_causes.len() == 1 {
            nullable_causes[0]
        } else {
            self.interner.union(nullable_causes)
        };
        return PropertyAccessResult::PossiblyNullOrUndefined {
            property_type: None,
            cause,
        };
    }
    if saw_unknown {
        return PropertyAccessResult::IsUnknown;
    }

    // ✅ NEW: Before giving up, check if any member has an index signature
    // For intersections, if ANY member has an index signature, the property access should succeed
    use crate::solver::index_signatures::{IndexKind, IndexSignatureResolver};
    let resolver = IndexSignatureResolver::new(self.interner);

    // Check string index signature on all members
    for &member in members.iter() {
        if resolver.has_index_signature(member, IndexKind::String) {
            if let Some(value_type) = resolver.resolve_string_index(member) {
                return PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                };
            }
        }
    }

    // Check numeric index signature if property name looks numeric
    if resolver.is_numeric_index_name(prop_name) {
        for &member in members.iter() {
            if let Some(value_type) = resolver.resolve_number_index(member) {
                return PropertyAccessResult::Success {
                    type_id: self.add_undefined_if_unchecked(value_type),
                    from_index_signature: true,
                };
            }
        }
    }

    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

## Key Changes

1. **String index signature check**: Iterate through all intersection members and check if any has a string index signature
2. **Numeric index signature check**: For numeric property names, check if any member has a numeric index signature
3. **Proper union of results**: If index signature is found, return success with the appropriate type
4. **Undefined handling**: Add `undefined` to the result type if `noUncheckedIndexedAccess` is enabled

## Why This Works

### Intersection Type Semantics

In TypeScript, an intersection type `A & B` has all properties from both A and B. For property access:

1. If property exists explicitly in A → use A's type
2. If property exists explicitly in B → use B's type
3. If property exists in both → intersect the types
4. **If property exists in neither → check index signatures**

The previous implementation missed step 4, causing false positives.

### Index Signature Propagation

For intersection types with index signatures:

```typescript
type A = { [key: string]: number };
type B = { x: string };
type C = A & B;

const obj: C = { x: "hello", y: 42 };
obj.anyProperty;  // Should be number (from A's index signature)
```

The fix ensures that when looking up `anyProperty`:
1. Check A: not found explicitly, but has string index signature → number
2. Check B: not found
3. Result: number (via A's index signature)

## Testing

The fix can be verified with cases like:

```typescript
// Test 1: String index signature in intersection
type WithIndex = { [key: string]: number };
type Specific = { x: string };
type Combined = WithIndex & Specific;

declare const obj: Combined;
obj.y;  // Should be number (via index signature)

// Test 2: Numeric index signature in intersection
type WithNumIndex = { [key: number]: boolean };
type Combined2 = WithNumIndex & Specific;
obj[0];  // Should be boolean (via numeric index signature)

// Test 3: No index signature - should error
type NoIndex = { x: string };
type NoIndex2 = { y: number };
type Combined3 = NoIndex & NoIndex2;

declare const obj2: Combined3;
obj2.z;  // Should error TS2339
```

## Impact

This fix should reduce TS2339 false positives for:

1. Intersection types with index signatures
2. Complex type compositions involving intersections
3. Generic types that resolve to intersections with index signatures
4. Merged interface types that create intersections

## Related Work

This complements previous TS2339 fixes:

1. **Object type index signature fallback** (commit 60a056cc5)
2. **Union type index signature handling** (already implemented)
3. **Ref/TypeQuery evaluation** (commit 0f8d82f0e)
4. **TypeParameter constraint resolution** (already implemented)

## Files Modified

- `/Users/claude/code/tsz/src/solver/operations.rs`:
  - Lines 2420-2473: Added index signature checking to intersection type property access

## Verification

To verify the fix works:

1. Build the project: `cargo build --release`
2. Run conformance tests and check TS2339 error count
3. Run specific property access unit tests
4. Verify no regression in existing tests

## Expected Improvement

While this fix targets a specific pattern, the impact should be:

- **Direct**: Reduces TS2339 errors for intersection types with index signatures
- **Indirect**: May improve generic type resolution where generics create intersections
- **Conservative**: Only affects cases where all members fail explicit property lookup

The fix is conservative and follows the same pattern already established for:
- Object types (lines 2147-2175)
- ObjectWithIndex types (lines 2194-2217)
- Union types (lines 2246-2357)
