# TS2339 Fix Analysis - Property Access False Positives

## Current State

**Extra Errors**: 470 TS2339 errors (Property 'X' does not exist on type 'Y')

## Root Cause Analysis

After analyzing the property access logic in `src/solver/operations.rs`, I've identified several key issues:

### 1. **Index Signature Fallback Not Implemented for Regular Objects**

**Location**: `src/solver/operations.rs:2084-2101`

**Issue**: When a property is accessed on a plain `Object` (not `ObjectWithIndex`), the code only checks for declared properties and apparent members. It doesn't fall back to checking for a string index signature.

```rust
TypeKey::Object(shape_id) => {
    let shape = self.interner.object_shape(shape_id);
    let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
    if let Some(prop) = self.lookup_object_property(shape_id, &shape.properties, prop_atom)
    {
        return PropertyAccessResult::Success {
            type_id: self.optional_property_type(prop),
            from_index_signature: false,
        };
    }
    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
        return result;
    }
    // ❌ MISSING: Should check for index signatures here!
    PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    }
}
```

**Expected Behavior**: If the object has a string index signature (even in a plain `Object`), property access should fall back to it when the property isn't explicitly declared.

**Fix**: Add index signature resolution using `IndexSignatureResolver`.

---

### 2. **Union Type Property Access Too Strict**

**Location**: `src/solver/operations.rs:2217-2223`

**Issue**: When checking property access on union types, if ANY member doesn't have the property, the entire access fails with `PropertyNotFound`.

```rust
// If any non-nullable member is missing the property, it's a PropertyNotFound error
_ => {
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

**Expected Behavior**: TypeScript allows property access on unions when:
1. The property exists in ALL union members, OR
2. The property exists in some members via index signatures

**Current Problem**: The code doesn't properly handle cases where different union members have the property via different mechanisms (explicit property vs index signature).

---

### 3. **Intersection Type Property Access Incomplete**

**Location**: `src/solver/operations.rs:2268-2337`

**Issue**: Intersection property access collects results from all members, but:
1. It doesn't handle index signatures properly
2. It returns `PropertyNotFound` when NO members have the property, but should continue checking

```rust
if results.is_empty() {
    if !nullable_causes.is_empty() {
        // ... handle nullable causes
    }
    if saw_unknown {
        return PropertyAccessResult::IsUnknown;
    }
    // ❌ PREMATURE: Should check index signatures before giving up
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

---

### 4. **Declaration Merging Not Checked**

**Issue**: When interfaces are merged (same name, multiple declarations), the property access logic doesn't check all merged declarations.

**Expected Behavior**: Property access on a merged interface should find properties from ALL merged declarations.

**Current Problem**: The solver only sees the merged shape but may not have all properties from all declarations properly indexed.

---

### 5. **Array/Tuple Property Access Missing Index Signature Check**

**Location**: `src/solver/operations.rs:2402-2410`

**Issue**: Arrays and tuples have implicit numeric (and for arrays, string) index signatures, but the code doesn't properly fall back to them.

```rust
TypeKey::Array(_) => {
    let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));
    self.resolve_array_property(obj_type, prop_name, prop_atom)
}
```

**Expected Behavior**: Array properties should:
1. Check array methods (length, push, pop, etc.)
2. Fall back to numeric index signature for numeric properties
3. Fall back to string index signature for array types

---

## High-Impact Fixes

### Priority 1: Fix Object Index Signature Fallback

**Impact**: High - Many objects have index signatures that aren't being checked

**Fix Location**: `src/solver/operations.rs:2084-2101`

```rust
TypeKey::Object(shape_id) => {
    let shape = self.interner.object_shape(shape_id);
    let prop_atom = prop_atom.unwrap_or_else(|| self.interner.intern_string(prop_name));

    // Check explicit properties
    if let Some(prop) = self.lookup_object_property(shape_id, &shape.properties, prop_atom)
    {
        return PropertyAccessResult::Success {
            type_id: self.optional_property_type(prop),
            from_index_signature: false,
        };
    }

    // Check apparent members
    if let Some(result) = self.resolve_object_member(prop_name, prop_atom) {
        return result;
    }

    // ✅ NEW: Check for index signatures
    use crate::solver::index_signatures::IndexSignatureResolver;
    let resolver = IndexSignatureResolver::new(self.interner);

    // Try string index signature
    if let Some(value_type) = resolver.resolve_string_index(obj_type) {
        // Check if property name is valid for string index
        // (most string properties are valid)
        return PropertyAccessResult::Success {
            type_id: self.add_undefined_if_unchecked(value_type),
            from_index_signature: true,
        };
    }

    // Try numeric index signature if property name is numeric
    if resolver.is_numeric_index_name(prop_name) {
        if let Some(value_type) = resolver.resolve_number_index(obj_type) {
            return PropertyAccessResult::Success {
                type_id: self.add_undefined_if_unchecked(value_type),
                from_index_signature: true,
            };
        }
    }

    PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    }
}
```

---

### Priority 2: Fix Union Type Index Signature Handling

**Impact**: High - Union types are very common in TypeScript

**Fix Location**: `src/solver/operations.rs:2160-2266`

The union handling needs to:
1. Collect properties from ALL members
2. Track which members used index signatures
3. Only fail if NO member has the property (explicit OR index signature)

---

### Priority 3: Fix Intersection Type Index Signature Handling

**Impact**: Medium - Intersection types are less common but still important

**Fix Location**: `src/solver/operations.rs:2268-2337`

Similar to unions, intersections should:
1. Collect properties from ALL members
2. Check index signatures if no explicit properties found
3. Combine the results properly

---

### Priority 4: Array Property Index Signature Fallback

**Impact**: Medium - Arrays are very common

**Fix Location**: Check `resolve_array_property` function

Ensure that array property access:
1. Checks array methods first (length, etc.)
2. Falls back to numeric index for numeric property names
3. Falls back to string index for other properties

---

## Testing Strategy

1. **Create test cases** for each pattern:
   - Index signature property access
   - Union type property access with index signatures
   - Intersection type property access
   - Array property access via index signatures
   - Declaration merging property access

2. **Verify fixes** reduce TS2339 extra error count

3. **Run conformance tests** to ensure no regressions

---

## Expected Impact

With these fixes, we should see:
- **Significant reduction** in TS2339 extra errors (target: < 100)
- **Improved accuracy** of property access resolution
- **Better compatibility** with TypeScript's behavior

---

## Implementation Plan

1. ✅ Analyze property access logic
2. ⏳ Implement Priority 1 fix (Object index signature fallback)
3. ⏳ Implement Priority 2 fix (Union index signature handling)
4. ⏳ Implement Priority 3 fix (Intersection index signatures)
5. ⏳ Implement Priority 4 fix (Array index signatures)
6. ⏳ Create comprehensive tests
7. ⏳ Run conformance tests and verify improvement
8. ⏳ Commit fixes

---

## Files to Modify

1. `src/solver/operations.rs` - Main property access logic
2. `src/solver/index_signatures.rs` - May need enhancements
3. Test files for verification

---

## References

- TypeScript spec: Property access resolution
- `src/solver/element_access.rs` - Element access evaluator (good reference)
- `src/solver/index_signatures.rs` - Index signature resolution
- Conformance test results for baseline
