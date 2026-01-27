# TS2304 Duplicate Error Analysis

## Problem

TS2304 errors are being emitted 4 times for class property type annotations with undefined names.

Example:
```typescript
class C<T> {
    foo: asdf;  // TS2304 emitted 4 times
    bar: C<asdf>;  // TS2304 emitted 4 times
}
```

## Root Cause

The function `get_type_from_type_node` in `/Users/mohsenazimi/code/tsz/src/checker/state.rs` (line 8060) triggers error emission in two phases:

1. **Pre-lowering check** (line 8085): `check_type_for_missing_names(idx)`
   - Walks the type tree recursively
   - Calls `get_type_from_type_reference` which emits TS2304

2. **Type lowering** (line 8098): `lowering.lower_type(idx)`
   - Lowers the type node to TypeId
   - Also calls type resolution which emits TS2304

For class properties specifically, both paths are triggered, resulting in duplicate errors.

## Call Stack

For property type annotation `foo: asdf`:

```
get_type_of_class_member (type_computation.rs:573-585)
  → get_type_from_type_node(prop.type_annotation) (line 580)
    → check_type_for_missing_names(idx) (line 8085)  ← EMITS TS2304
    → lowering.lower_type(idx) (line 8098)
      → TypeLowering::lower_type
        → resolve_type_symbol_for_lowering
          → resolve_type_symbol_for_lowering
            → get_type_from_type_reference (line 8067)  ← EMITS TS2304 AGAIN
```

## Solution Options

### Option 1: Track emitted errors per node
Add a HashSet to track which nodes have already emitted TS2304 errors and suppress duplicates.

Pros:
- Simple to implement
- No behavior change for valid code
- Fixes the immediate problem

Cons:
- Adds memory overhead
- Doesn't address the architectural issue

### Option 2: Remove pre-check, rely only on lowering
Remove the `check_type_for_missing_names` call and rely solely on type lowering to emit errors.

Pros:
- Eliminates duplicate work
- Cleaner architecture

Cons:
- May miss errors in some edge cases
- Need to verify all error paths

### Option 3: Split check and emit phases
Separate error detection from error emission:
- Phase 1: Detect all errors without emitting
- Phase 2: Emit unique errors

Pros:
- Most robust solution
- Prevents all duplicates

Cons:
- Most complex to implement
- Requires significant refactoring

## Recommended Solution

**Option 1** with a slight refinement: Track errors only within the same type resolution context.

Add a `HashSet<NodeIndex>` to track nodes that have already had TS2304 emitted during the current `get_type_from_type_node` call.
