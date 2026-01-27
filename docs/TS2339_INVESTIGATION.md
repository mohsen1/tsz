# TS2339 Investigation Report

## Current Status

**Extra Errors**: ~8,178 TS2339 errors (Property 'X' does not exist on type 'Y')
**Goal**: Reduce TS2339 false positives to match TypeScript's behavior

## Investigation Summary

### What Works Well

1. **Index Signature Fallback** (Implemented in commit 60a056cc5)
   - Object types now check for string/numeric index signatures before failing
   - ObjectWithIndex types properly handle both string and numeric index signatures
   - Array types have numeric index fallback for property access
   - **Impact**: Significantly reduced TS2339 errors for index signature cases

2. **Type Resolution** (Implemented in commit 0f8d82f0e)
   - `resolve_type_for_property_access` properly handles:
     - TypeKey::Ref and TypeKey::TypeQuery
     - TypeKey::Application
     - TypeKey::Conditional, Mapped, IndexAccess, KeyOf
     - TypeKey::TypeParameter and Infer
     - Union and Intersection types
   - **Impact**: Reduces false positives for generic types, type aliases, and complex type expressions

3. **Apparent Members**
   - Well-implemented for built-in types (Object, String, Number, etc.)
   - Provides fallback methods like toString, valueOf, etc.

### Remaining Issues

Based on code analysis, here are the main sources of remaining TS2339 false positives:

#### 1. Unresolved Type References

**Location**: `src/solver/operations.rs:2520-2590`

When `evaluate_type` doesn't change a Ref/TypeQuery/Conditional type, we return:
```rust
PropertyAccessResult::Success {
    type_id: TypeId::ANY,
    from_index_signature: false,
}
```

**Problem**: While this prevents false positives in the solver, the checker may still emit TS2339 before the solver is even called.

**Evidence**: Line 1070 in `type_computation.rs` calls `property_access_type` WITHOUT resolving first:
```rust
let result = self.ctx.types.property_access_type(object_type_for_access, &property_name);
```

**Fix Attempted**: Adding `resolve_type_for_property_access` before this call actually INCREASED errors from 421x to 468x in 50-test sample, suggesting the issue is more complex.

#### 2. Union Type Property Access

**Location**: `src/solver/operations.rs:2256`

```rust
// If any non-nullable member is missing the property, it's a PropertyNotFound error
_ => {
    return PropertyAccessResult::PropertyNotFound {
        type_id: obj_type,
        property_name: prop_atom,
    };
}
```

**Analysis**: This is actually CORRECT behavior per TypeScript's semantics. If ANY member of a union doesn't have the property, the entire property access fails.

**However**: The issue might be that we're not properly finding properties via index signatures on ALL union members before failing.

#### 3. Declaration Merging

**Issue**: When interfaces are merged across multiple declarations, the property access logic may not find all properties.

**Location**: `src/checker/state.rs:6893-6905` handles merged class+namespace, but there's no equivalent for interface merging in the solver.

**Impact**: Properties from later interface declarations may not be found.

#### 4. Module Namespace Members

**Issue**: Properties on module namespaces (e.g., `import("./module").exportedMember`) may not be resolved correctly.

**Location**: `src/checker/state.rs:6929-6939` handles namespace references, but the property access may fail if the namespace type isn't properly structured.

#### 5. Type Parameter Constraints

**Issue**: When accessing properties on type parameters with constraints, we may not properly check the constraint for the property.

**Location**: `src/solver/operations.rs` doesn't have special handling for TypeParameter before calling resolve_property_access_inner.

**Example**:
```typescript
function foo<T extends { x: number }>(obj: T) {
    return obj.x;  // Should work, may fail in TSZ
}
```

### High-Priority Fixes

#### Priority 1: Fix Element Access Type Resolution

**File**: `src/checker/type_computation.rs:1070`

**Current Code**:
```rust
let result = self.ctx.types.property_access_type(object_type_for_access, &property_name);
```

**Should Be**:
```rust
let resolved_type = self.resolve_type_for_property_access(object_type_for_access);
let result = self.ctx.types.property_access_type(resolved_type, &property_name);
```

**Caveat**: Initial test showed this increased errors. Needs investigation.

#### Priority 2: Improve Union Type Index Signature Checking

**File**: `src/solver/operations.rs:2267-2303`

**Issue**: When iterating union members, we call `resolve_property_access_inner` which should handle index signatures. However, we may need to verify this is working correctly.

**Investigation Needed**: Add logging to see which union members are returning PropertyNotFound and why.

#### Priority 3: Add Interface Merging Support

**File**: `src/solver/operations.rs`

**Idea**: When looking up properties on an Object type that represents an interface, check if the symbol has multiple declarations and merge their properties.

**Complexity**: High - requires binder integration in the solver

#### Priority 4: Better Type Parameter Handling

**File**: `src/solver/operations.rs`

**Idea**: Before failing on TypeParameter, check the constraint for the property.

**Example**:
```rust
TypeKey::TypeParameter(info) => {
    // Try the constraint first
    if let Some(constraint) = info.constraint {
        return self.resolve_property_access_inner(constraint, prop_name, prop_atom);
    }
    PropertyAccessResult::PropertyNotFound { ... }
}
```

**Current State**: TypeParameter is already handled by `evaluate_type`, but may not be working in all cases.

### Test Cases for Verification

1. **Index Signature on Plain Object**:
   ```typescript
   interface A { [key: string]: number }
   declare const a: A;
   a.anyProperty;  // Should work
   ```

2. **Generic with Constraint**:
   ```typescript
   function foo<T extends { x: number }>(obj: T) {
       return obj.x;  // Should work
   }
   ```

3. **Union with Index Signature**:
   ```typescript
   type A = { x: number };
   type B = { [key: string]: number };
   declare const obj: A | B;
   obj.x;  // Should work (in A)
   obj.y;  // Should ERROR (not in A, even though B has index sig)
   ```

4. **Declaration Merging**:
   ```typescript
   interface Box { width: number; }
   interface Box { height: number; }
   declare const b: Box;
   b.width;   // Should work
   b.height;  // Should work
   ```

5. **Module Namespace**:
   ```typescript
   // module.ts
   export { x };
   // main.ts
   import { x } from "./module";
   x;  // Should work
   ```

### Next Steps

1. **Investigate the 421→468 regression**: Why did adding type resolution increase errors?
   - Are we now finding real errors that were being missed?
   - Or are we being too strict somewhere?

2. **Add diagnostic logging**: Temporarily add logging to see exactly which properties are failing and why

3. **Create targeted test cases**: Build specific test cases for each remaining issue

4. **Fix incrementally**: Address each issue separately with proper testing

5. **Run conformance**: After each fix, verify TS2339 count decreases

## References

- `/Users/mohsenazimi/code/tsz/docs/TS2339_FIX_ANALYSIS.md` - Original fix analysis
- Commit 60a056cc5 - Index signature fallback implementation
- Commit 0f8d82f0e - Type resolution improvements
- `src/solver/operations.rs` - Main property access logic
- `src/checker/state.rs:6864` - resolve_type_for_property_access
- `src/checker/type_computation.rs:510` - Element access type checking
