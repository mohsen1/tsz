# TS2339 Property Access - Comprehensive Investigation and Fixes

## Overview

This document summarizes the current state of TS2339 "Property does not exist on type" error handling in TSZ, identifies remaining issues, and documents implemented fixes.

**Current Status**: ~8,178 extra TS2339 errors (as of session summary)
**Target**: Reduce to < 100 extra errors

---

## Previously Implemented Fixes

### 1. Object Type Index Signature Fallback (Commit 60a056cc5)

**Location**: `src/solver/operations.rs:2147-2175`

Added index signature resolution for plain Object types that don't explicitly declare properties but have string or numeric index signatures.

```rust
TypeKey::Object(shape_id) => {
    // ... explicit property lookup ...
    // ... apparent members check ...

    // Check for index signatures using IndexSignatureResolver
    let resolver = IndexSignatureResolver::new(self.interner);

    // Try string index signature
    if resolver.has_index_signature(obj_type, IndexKind::String) {
        if let Some(value_type) = resolver.resolve_string_index(obj_type) {
            return PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                from_index_signature: true,
            };
        }
    }

    // Try numeric index signature
    if resolver.is_numeric_index_name(prop_name) {
        if let Some(value_type) = resolver.resolve_number_index(obj_type) {
            return PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                from_index_signature: true,
            };
        }
    }

    PropertyAccessResult::PropertyNotFound { ... }
}
```

**Impact**: Fixed property access on objects with index signatures that don't explicitly declare the property.

### 2. Type Reference Resolution (Commit 0f8d82f0e)

**Location**: `src/solver/operations.rs:2573-2712`

Added evaluation for type constructors that need resolution before property access:

- **Ref types**: Resolve symbol references to their structural forms
- **TypeQuery types**: Resolve `typeof` queries to actual types
- **Application types**: Evaluate generic type applications
- **Conditional types**: Resolve conditional type expressions
- **IndexAccess types**: Evaluate indexed access types
- **KeyOf types**: Resolve keyof expressions
- **TypeParameter/Infer types**: Fall back to constraints

```rust
TypeKey::Ref(_) => {
    let evaluated = evaluate_type(self.interner, obj_type);
    if evaluated != obj_type {
        self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
    } else {
        // Try apparent members, return ANY on failure
        ...
    }
}
```

**Impact**: Fixed property access on type aliases, interfaces, classes, typeof queries, and generic types.

### 3. Union Type Property Access with Index Signatures

**Location**: `src/solver/operations.rs:2246-2357`

Union types correctly handle index signatures:

```rust
TypeKey::Union(members) => {
    // Filter out ANY, ERROR, UNKNOWN
    // Partition into nullable and non-nullable members
    for &member in non_unknown_members.iter() {
        match self.resolve_property_access_inner(member, prop_name, Some(prop_atom)) {
            PropertyAccessResult::Success { type_id, from_index_signature } => {
                valid_results.push(type_id);
                if from_index_signature {
                    any_from_index = true;
                }
            }
            PropertyAccessResult::PropertyNotFound { .. } => {
                // If ANY member doesn't have the property, union access fails
                return PropertyAccessResult::PropertyNotFound { ... };
            }
        }
    }

    // Union of all successful result types
    PropertyAccessResult::Success {
        type_id: self.interner.union(valid_results),
        from_index_signature: any_from_index,
    }
}
```

**Behavior**: Property access on unions succeeds only if ALL non-nullable members have the property (either explicitly or via index signature).

**Impact**: Correctly handles unions where all members support the property access.

### 4. Checker-Level Type Resolution

**Location**: `src/checker/type_computation.rs:1077`

The checker resolves types before calling the solver:

```rust
let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
let result = self.ctx.types.property_access_type(resolved_type, &property_name);
```

**Location**: `src/checker/state.rs:6903-7067`

The `resolve_type_for_property_access` function handles:

- Ref types (symbol references)
- TypeQuery types (typeof queries)
- Application types (generic applications)
- TypeParameter/Infer types (constraints)
- Conditional/Mapped/IndexAccess/KeyOf types
- Union types (resolves all members)
- Intersection types (resolves all members)
- ReadonlyType (unwraps)
- Function/Callable types (adds Function interface)

**Impact**: Ensures complex types are resolved to their structural forms before property lookup.

---

## New Fix: Intersection Type Index Signature Fallback

### Problem Identified

Intersection types were not checking for index signatures when no explicit property was found on any member.

**Example**:

```typescript
type A = { [key: string]: number };
type B = { x: string };
type C = A & B;

const obj: C = { x: "hello" };
obj.y;  // Should be number via A's index signature, but was failing
```

### Solution Implemented

**Location**: `src/solver/operations.rs:2420-2473`

Added index signature resolution for intersection types before returning `PropertyNotFound`:

```rust
if results.is_empty() {
    // ... handle nullable causes and unknown ...

    // NEW: Check if any member has an index signature
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

    return PropertyAccessResult::PropertyNotFound { ... };
}
```

**Why This Works**: For intersection types `A & B`, if ANY member has an index signature, property access should succeed using that signature.

**Impact**: Reduces TS2339 false positives for intersection types with index signatures.

---

## Remaining Issues and Potential Improvements

### 1. Array/Tuple Property Access

**Status**: Already well implemented

Arrays and tuples correctly handle:
- Explicit properties (length, push, pop, etc.)
- Numeric index signatures via `resolve_array_property`
- String index signatures (for array methods)

**Location**: `src/solver/operations.rs:2550-3168`

No changes needed.

### 2. Module/Namespace Property Access

**Status**: Well implemented

Namespace resolution is comprehensive:

**Location**: `src/checker/type_checking.rs:8370-8456`

- Handles direct exports
- Follows re-export chains
- Resolves aliases
- Handles enum members
- Supports merged class+namespace Callable types

The checker calls `resolve_namespace_value_member` before falling back to general property access, ensuring namespace-specific logic is applied first.

**Potential Issue**: If a namespace has an index signature, it's not currently checked. However, namespaces typically don't have index signatures in TypeScript, so this is likely not a real issue.

### 3. Generic Type Resolution

**Status**: Good, but may have edge cases

Application types are evaluated before property access:

```rust
TypeKey::Application(_) => {
    let evaluated = evaluate_type(self.interner, obj_type);
    if evaluated != obj_type {
        self.resolve_property_access_inner(evaluated, prop_name, prop_atom)
    } else {
        // Fall back to apparent members
        ...
    }
}
```

**Potential Issue**: If `evaluate_type` fails to resolve a generic (returns the same type), we fall back to apparent members and then return `PropertyNotFound`. Some complex generic instantiations might not be fully resolved.

**Mitigation**: The checker's `evaluate_application_type` in `resolve_type_for_property_access` should handle most cases.

### 4. Declaration Merging

**Status**: Partially implemented

**Location**: `src/checker/state.rs:6932-6944`

Merged class+namespace symbols are handled:

```rust
if symbol.flags & symbol_flags::CLASS != 0
    && symbol.flags & symbol_flags::MODULE != 0
{
    let ctor_type = self.get_class_constructor_type(class_idx, class_data);
    return self.resolve_type_for_property_access_inner(ctor_type, visited);
}
```

**Potential Issue**: Interface merging (multiple declarations of the same interface) may not be fully handled. The solver sees a merged shape but properties from later declarations might not be indexed.

**Investigation Needed**: Check if interface declarations are properly merged during binding.

### 5. Circular Type References

**Status**: Protected

Both the checker and solver have cycle detection:

```rust
if !visited.insert(type_id) {
    return type_id;  // Prevent infinite recursion
}
```

**Status**: Not an issue.

### 6. Error Cascade and False Positives

**Observation**: Some types return `ANY` when evaluation fails to avoid false positives:

```rust
TypeKey::Conditional(_) => {
    let evaluated = evaluate_type(self.interner, obj_type);
    if evaluated == obj_type {
        // Can't evaluate - return ANY to avoid false positives
        PropertyAccessResult::Success {
            type_id: TypeId::ANY,
            from_index_signature: false,
        }
    }
}
```

**Trade-off**: This prevents false positives but may hide real errors. Changing this behavior requires careful testing.

---

## Summary of Changes

### Files Modified

1. **`src/solver/operations.rs`**
   - Lines 2420-2473: Added index signature checking to intersection type property access

### New Documentation

1. **`docs/TS2339_INTERSECTION_FIX.md`**
   - Detailed explanation of the intersection type fix
   - Examples and test cases
   - Expected impact

---

## Testing and Verification

### Recommended Test Cases

```typescript
// Test 1: Intersection with string index signature
type WithStringIndex = { [key: string]: number };
type WithX = { x: string };
type Combined1 = WithStringIndex & WithX;

declare const obj1: Combined1;
obj1.anyProperty;  // Should be number

// Test 2: Intersection with numeric index signature
type WithNumIndex = { [key: number]: boolean };
type Combined2 = WithNumIndex & WithX;
obj1[42];  // Should be boolean

// Test 3: Union - all members must have property
type A = { x: number };
type B = { [key: string]: string };
type Union = A | B;

declare const obj2: Union;
obj2.x;   // Should be number | string (exists in both)
obj2.y;   // Should ERROR (not in A, even though B has index sig)

// Test 4: Generic with constraint
function foo<T extends { x: number }>(obj: T) {
    return obj.x;  // Should work
}

// Test 5: Array with numeric property
const arr: number[] = [1, 2, 3];
arr[0];     // Should be number
arr["length"];  // Should be number (array property)
arr["any"];  // Should ERROR (arrays don't have string index sig)

// Test 6: Object with index signature
type Obj = { [key: string]: number; x: string };
declare const obj3: Obj;
obj3.x;      // Should be string (explicit property)
obj3.any;    // Should be number (index signature)
```

### Verification Steps

1. Build the project:
   ```bash
   cargo build --release
   ```

2. Run conformance tests and check TS2339 count:
   ```bash
   ./scripts/test.sh 2>&1 | grep "TS2339"
   ```

3. Run property access unit tests:
   ```bash
   cargo test --lib -- solver::operations::tests::test_property_access
   ```

4. Check for regressions in other areas

---

## Expected Impact

### Direct Impact

- **Intersection types with index signatures**: Should now work correctly
- **Complex type compositions**: Improved resolution for types that resolve to intersections

### Conservative Estimate

- **Expected reduction**: 100-500 TS2339 errors
- **Risk level**: Low (only affects intersection types)
- **Regression risk**: Minimal (only adds more property access paths)

### Long-term Improvements Needed

To reach < 100 TS2339 errors, additional work may be needed on:

1. Generic type resolution edge cases
2. Interface declaration merging
3. Complex conditional type evaluation
4. Error cascade reduction (reduce ANY returns)
5. Module/namespace edge cases

---

## References

- **Previous fixes**:
  - `docs/TS2339_FIX_SUMMARY.md` - Index signature and Ref/TypeQuery fixes
  - `docs/TS2339_INVESTIGATION.md` - Original investigation
  - `docs/TS2339_FIX_ANALYSIS.md` - Detailed analysis

- **Related code**:
  - `src/solver/operations.rs` - Main property access logic
  - `src/solver/index_signatures.rs` - Index signature resolution
  - `src/checker/state.rs` - Type resolution for property access
  - `src/checker/type_computation.rs` - Element access type checking
