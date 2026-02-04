# Session tsz-2: Class Type Resolution Fix

**Started**: 2026-02-04
**Goal**: Fix class type resolution to distinguish constructor types from instance types

## Problem Statement

Class identifiers are resolving to the wrong type in different contexts:
- **TYPE position** (e.g., `let x: Animal`): Should resolve to **Instance Type** (Object with properties)
- **VALUE position** (e.g., `const ctor = Animal`): Should resolve to **Constructor Type** (Callable with construct signatures)

**Current Bug**: TYPE position resolves to Constructor Type instead of Instance Type

### Impact

This bug causes `test_abstract_constructor_assignability` to fail:
```typescript
function createAnimal(Ctor: typeof Animal): Animal {
    return new Dog(); // ERROR: Dog instance (Object) not assignable to Animal constructor (Callable)
}
```

The return type annotation `: Animal` should be the Animal **instance** type, but it's currently the Animal **constructor** type.

## Root Cause Analysis

**Investigation Findings**:
- `get_type_of_symbol` caches Constructor Type in `ctx.symbol_types` for classes
- `resolve_lazy` looks up types from `ctx.symbol_types` via `DefId`
- Type annotations create `Lazy(DefId)` which resolve to the cached type
- **Result**: Both TYPE and VALUE position get the Constructor Type

**Evidence**:
- Source type: `ObjectWithIndex(ObjectShapeId(2))` - Dog instance (correct!)
- Target type: `Callable(CallableShapeId(5))` - Animal constructor (WRONG!)
- Expected: Both should be Object instance types

## Fix Strategy

### Architecture Change: Dual Type Cache

Modify `CheckerContext` to maintain separate caches for constructor and instance types:

1. **Add instance type cache** to `src/checker/context.rs`:
   ```rust
   pub symbol_instance_types: FxHashMap<SymbolId, TypeId>
   ```

2. **Update class type computation** in `src/checker/state_type_analysis.rs`:
   - When computing a class symbol, cache BOTH types:
     - `symbol_types[sym_id] = constructor_type` (for VALUE position)
     - `symbol_instance_types[sym_id] = instance_type` (for TYPE position)

3. **Update lazy resolution** in `src/checker/context.rs`:
   - `resolve_lazy`: Check if symbol is a class
   - If class: Return type from `symbol_instance_types`
   - Otherwise: Return type from `symbol_types`

## Implementation Tasks

1. [ ] Add `symbol_instance_types` cache to `CheckerContext`
2. [ ] Update `get_class_constructor_type` to populate both caches
3. [ ] Update `resolve_lazy` to prefer instance types for classes
4. [ ] Add tests for type vs value position class resolution
5. [ ] Verify `test_abstract_constructor_assignability` passes
6. [ ] Run full test suite to check for regressions

## Success Criteria

- [ ] `test_abstract_constructor_assignability` passes (0 errors)
- [ ] Unit tests: 365/365 passing (fixing the 2 abstract class failures)
- [ ] Conformance: 50% → 55%+ (class-related tests should pass)
- [ ] No regressions in parser tests (287/287)
- [ ] All work tested, committed, and synced

## Notes

- This is a focused architectural fix, not a parser session
- Timebox each task to 30 minutes to avoid scope creep
- If the fix requires broader changes, document and defer
- The nominal inheritance check already works (verified during investigation)
