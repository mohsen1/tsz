# TS2339 Union Property Access Fix - Implementation

## Summary

Fixed the TS2339 regression where union property access would incorrectly report "Property does not exist" when at least one union member had the property.

## The Problem

**Before the fix**: The union property access logic would immediately return `PropertyNotFound` if ANY single member didn't have the property. This was incorrect TypeScript behavior.

**Example of incorrect behavior**:
```typescript
type Cat = { purr: void };
type Dog = { bark: void };

function test(animal: Cat | Dog) {
    const x = animal.purr; // BEFORE: Would error because Dog doesn't have .purr
                           // AFTER: Works correctly because Cat has .purr
}
```

## The Solution

Changed the logic to:
1. **Skip** union members that don't have the property (instead of failing immediately)
2. Only return `PropertyNotFound` if **NO** non-nullable members have the property

## Implementation Details

**File**: `/Users/claude/code/tsz/src/solver/operations.rs`

**Lines changed**: 2334-2348

### Before (Incorrect)
```rust
PropertyAccessResult::PossiblyNullOrUndefined {
    property_type,
    cause,
} => {
    if let Some(t) = property_type {
        valid_results.push(t);
    }
    nullable_causes.push(cause);
}
// If any non-nullable member is missing the property, it's a PropertyNotFound error
_ => {
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

### After (Correct)
```rust
PropertyAccessResult::PossiblyNullOrUndefined {
    property_type,
    cause,
} => {
    if let Some(t) = property_type {
        valid_results.push(t);
    }
    nullable_causes.push(cause);
}
// PropertyNotFound or IsUnknown: skip this member, continue checking others
PropertyAccessResult::PropertyNotFound { .. }
| PropertyAccessResult::IsUnknown => {
    // Member doesn't have this property - skip it
}
```

### Additional Check Added
After the loop, added a check to return `PropertyNotFound` only when no members had the property:

```rust
// If no non-nullable members had the property, it's a PropertyNotFound error
if valid_results.is_empty() && nullable_causes.is_empty() {
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

## Test Results

Created test file `test_ts2339_simple.ts`:
```typescript
type Cat = { purr: void };
type Dog = { bark: void };

function test(animal: Cat | Dog) {
    const x = animal.purr; // ✅ Works: Cat has .purr
    const y = animal.bark; // ✅ Works: Dog has .bark
}
```

**Result**: ✅ No errors - fix working correctly!

Created test file `test_ts2339_union_fix.ts` with comprehensive cases:
- ✅ Property access works when at least one member has the property
- ✅ PropertyNotFound correctly reported when NO members have the property
- ✅ Nullable unions handled correctly (TS2531/TS2532 for null/undefined)

## Correct Behavior

The fix implements proper TypeScript union property access semantics:

1. **At least one member has property**: ✅ Allow access (union of all matching types)
2. **No members have property**: ❌ Report TS2339 PropertyNotFound
3. **Some members have property, some don't**: ✅ Allow access (skip non-matching members)
4. **Members with null/undefined**: Handle with PossiblyNullOrUndefined

## Verification

```bash
# Build succeeds with only warnings (no errors)
cargo build

# Test shows fix working correctly
cargo run --bin tsz -- test_ts2339_simple.ts
# Result: No errors ✅
```

## Impact

This fix resolves the TS2339 regression and restores correct TypeScript behavior for union property access, allowing developers to use discriminated unions and other union patterns without false positive errors.
