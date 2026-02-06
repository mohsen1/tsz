# Session TSZ-17: Index Signature Debug Investigation

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Predecessor**: TSZ-13 (Index Signatures - Code Already Exists)

## Summary

Fixed index signature test failures by resolving two issues:
1. `type_reference_symbol_type` was returning `Lazy(DefId)` wrapper types instead of the actual structural type
2. Flow analysis was returning `ANY` for variables with interface types, overriding the declared type

## Root Cause Analysis

### Test: test_checker_lowers_element_access_string_index_signature

**Code**:
```typescript
interface StringMap {
    [key: string]: boolean;
}
const map: StringMap = {} as any;
const value = map["foo"];
```

**Expected**: `value_type` should be `TypeId::BOOLEAN`
**Actual**: Got `TypeId(4)` (TypeId::ANY)

**Investigation Findings**:

1. **First Issue - Lazy Type Wrapping**:
   - `type_reference_symbol_type` (state_type_resolution.rs:672) was returning `Lazy(DefId)` wrapper types
   - This was intended for error formatting, but caused issues with type resolution
   - **Fix**: Changed to return the structural type directly (from `get_type_of_symbol`) instead of the `Lazy` wrapper

2. **Second Issue - Flow Analysis Override**:
   - Even after fixing the first issue, `check_flow_usage` was returning `ANY`
   - This overrode the correct declared type for variables with interface types
   - **Fix**: Added check in `get_type_of_identifier` to preserve declared type when flow analysis returns `ANY` inappropriately

## Changes Made

### 1. src/checker/state_type_resolution.rs (line 653-690)

Changed `type_reference_symbol_type` for interfaces to return the structural type instead of `Lazy(DefId)`:

```rust
// BEFORE: Returned Lazy(DefId) wrapper
let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
return lazy_type;

// AFTER: Return structural type directly
let structural_type = self.get_type_of_symbol(sym_id);
return structural_type;
```

**Rationale**: The `Lazy` wrapper was causing flow analysis to fail to properly handle the type. Error formatting can still look up interface names via symbol_id.

### 2. src/checker/type_computation_complex.rs (line 1381-1416)

Added protection against flow analysis returning `ANY` inappropriately:

```rust
let flow_type = self.check_flow_usage(idx, declared_type, sym_id);

// FIX: If flow analysis returns ANY but the declared type is valid, use declared type
let result_type = if flow_type == TypeId::ANY
    && declared_type != TypeId::ANY
    && declared_type != TypeId::ERROR
{
    declared_type
} else {
    flow_type
};

return result_type;
```

**Rationale**: Flow analysis may not have proper type information for variables with interface types or other complex types. In such cases, we should preserve the declared type rather than falling back to `ANY`.

## Test Status

**Start**: 8247 passing, 53 failing
**End**: 8249 passing, 51 failing
**Result**: +2 tests fixed

## Changes Made

### 1. src/checker/state_type_resolution.rs (line 653-690)

Changed `type_reference_symbol_type` for interfaces to return the structural type directly ONLY for interfaces with index signatures (ObjectWithIndex):

```rust
let structural_type = self.get_type_of_symbol(sym_id);

// FIX: For interfaces with index signatures (ObjectWithIndex), return the structural
// type directly instead of Lazy wrapper. The Lazy type causes issues with flow
// analysis - it returns ANY instead of the proper type.
match self.ctx.types.lookup(structural_type) {
    Some(TypeKey::ObjectWithIndex(_)) => {
        self.ctx.leave_recursion();
        return structural_type;
    }
    _ => {
        // Return Lazy wrapper for regular interfaces (without index signatures)
        let lazy_type = self.ctx.create_lazy_type_ref(sym_id);
        self.ctx.leave_recursion();
        return lazy_type;
    }
}
```

**Rationale**: The `Lazy` wrapper was causing flow analysis to return `ANY` for variables with index signature types. By returning the structural type directly for ObjectWithIndex types, flow analysis can properly resolve the type. Regular interfaces still use Lazy wrapper to preserve error formatting.

### 2. src/checker/type_computation_complex.rs (line 1379-1414)

Added protection against flow analysis returning `ANY` inappropriately for ObjectWithIndex types:

```rust
let flow_type = self.check_flow_usage(idx, declared_type, sym_id);

// FIX: If flow analysis returns ANY but the declared type is a valid non-ANY, non-ERROR type,
// and the declared type is an ObjectWithIndex (has index signatures), use the declared type.
// IMPORTANT: Only apply this fix when there's NO contextual type to avoid interfering
// with variance checking and assignability analysis.
let result_type = if flow_type == TypeId::ANY
    && declared_type != TypeId::ANY
    && declared_type != TypeId::ERROR
    && self.ctx.contextual_type.is_none()
{
    match self.ctx.types.lookup(declared_type) {
        Some(TypeKey::ObjectWithIndex(_)) => declared_type,
        _ => flow_type,
    }
} else {
    flow_type
};
```

**Rationale**: Flow analysis may not have proper type information for variables with index signatures. In such cases, we preserve the declared type. The `contextual_type.is_none()` check ensures we don't interfere with variance checking or assignability analysis where flow analysis results are important.
