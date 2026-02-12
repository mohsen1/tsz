# Object Type Assignability Issue

## Problem
We're emitting false positive TS2322/TS2740 errors when assigning values to the global `Object` type.

## Affected Tests
- ~52 false positive TS2322 tests
- Example: `interfaceWithPropertyOfEveryType.ts`

## Root Cause
The global `Object` interface from lib.d.ts is being treated like a regular interface with structural property checking. When checking if `{}` is assignable to `Object`, we're requiring `{}` to have all properties like `toString`, `valueOf`, `propertyIsEnumerable`, etc.

## Expected Behavior (per TypeScript)
According to TypeScript semantics and our unit tests:
- ALL non-nullish values are assignable to the global `Object` interface
- This includes: `{}`, `{ x: 1 }`, primitives, functions, arrays
- Only `null` and `undefined` (in strict mode) should not be assignable

## Current Test Status
Unit tests PASS (`test_object_trifecta_*` in crates/tsz-solver/src/tests):
- These tests use `interner.lazy(def_id)` to wrap the Object interface
- Primitives and objects ARE correctly assignable to this wrapped type

Conformance tests FAIL:
- When `Object` is resolved from lib.d.ts via `resolve_lib_type_by_name("Object")`
- The resolved type is not being recognized as the special global Object

## Test Cases
```typescript
// All should be OK
var o1: Object = {};           // Currently FAILS: TS2740
var o2: Object = { x: 1 };     // Currently FAILS: TS2353
var o3: Object = "hello";      // OK (primitive apparent shape works)
var o4: Object = 42;           // OK
var o5: Object = function() {}; // Currently FAILS: TS2322
var o6: Object = [1, 2, 3];    // Currently FAILS: TS2322
```

## Investigation Findings

### Where Object is Resolved
- `crates/tsz-checker/src/type_checking.rs`: Resolves Object via `resolve_lib_type_by_name("Object")`
- Associates with `IntrinsicKind::Object` in type environment

### Subtype Checking
- `crates/tsz-solver/src/subtype.rs`: Has `apparent_primitive_shape_for_type` for primitives
- Unit tests show the logic exists for `Lazy(DefId)` wrapped Object types
- But direct Object interface types from lib.d.ts aren't being handled specially

### The Disconnect
1. Unit tests create Object as: `interner.lazy(def_id)` where def_id points to Object interface
2. Real code resolves Object from lib.d.ts, may return different type representation
3. Special handling for "global Object" needs to work regardless of representation

## Proposed Fix (Not Yet Implemented)

Need to identify when target type is the global `Object` and apply special rules:

```rust
// In check_subtype_inner or similar
fn is_global_object_type(&self, type_id: TypeId) -> bool {
    // Check if this is the Object type from lib.d.ts
    // Could check:
    // 1. Is it registered in type_env as IntrinsicKind::Object?
    // 2. Is it a Lazy/DefId that resolves to the Object symbol?
    // 3. Is it marked specially during lib resolution?
}

// When checking assignability
if self.is_global_object_type(target) {
    // ALL non-nullish types are assignable to Object
    return !matches!(source, TypeId::NULL | TypeId::UNDEFINED);
}
```

## Related Code Locations
- `crates/tsz-solver/src/subtype.rs`: Main subtype checking
- `crates/tsz-checker/src/type_checking.rs`: Object type resolution
- `crates/tsz-solver/src/tests/subtype_tests.rs`: test_object_trifecta_* tests
- `crates/tsz-solver/src/types.rs`: TypeId::OBJECT constant

## Impact
Fixing this would resolve ~50+ false positive tests and is a high-priority issue for conformance.

## Next Steps
1. Identify how to reliably detect "global Object" type across different representations
2. Add special case in subtype checker for non-nullish → Object
3. Verify unit tests still pass
4. Run conformance tests to measure improvement
5. Document the Object/object/{} trifecta behavior
