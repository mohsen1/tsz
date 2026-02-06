# Session tsz-3: Object Literal Freshness - Investigation In Progress

**Started**: 2026-02-06
**Status**: 🔍 INVESTIGATING (Complex bug, requires deeper analysis)
**Predecessor**: Index Access Type Evaluation (Already Implemented)

## Bug Summary

Object literal freshness widening is not working correctly. Variables declared with object literals should have their freshness stripped, but subsequent uses still trigger excess property checks.

### Expected Behavior
```typescript
let x = { a: 1, b: 2 };  // Freshness stripped
let y: { a: number } = x;  // Should PASS (x is non-fresh)
```

### Actual Behavior
Error: "Object literal may only specify known properties, and 'b' does not exist"

## Investigation Results

### Root Cause Identified

Debug output revealed:
```
DEBUG: var decl final_type before widen: TypeId(134)  (FRESH)
DEBUG: var decl final_type after widen: TypeId(114)  (NON-FRESH)
DEBUG: init_type for assignment check: TypeId(134), is_fresh: true  <-- BUG!
```

The widened type `TypeId(114)` is correctly stored in `symbol_types`, but `get_type_of_node` returns the original fresh type `TypeId(134)`.

### Cache Poisoning Issue

The problem is in the `node_types` cache:
1. When processing `let x = { a: 1, b: 2 }`, the object literal is computed and cached with FRESH type
2. `widen_freshness` creates a new TypeId with NON-FRESH type
3. The NON-FRESH type is stored in `symbol_types`
4. But `node_types` cache already has the FRESH type
5. When accessing `x` later, the cached FRESH type is returned

### Attempted Fix

Updated `check_variable_declaration` in `src/checker/state_checking.rs` to:
- Set `node_types[decl_idx]` to widened type
- Set `node_types[var_decl.name]` to widened type
- Remove `node_types[initializer]` to invalidate stale cache

**Result**: Tests still failing. The issue is more complex - there are multiple nodes involved (declaration node, name node, identifier expression nodes) and they may not all be updated correctly.

## Files Examined

- `src/solver/freshness.rs` - `widen_freshness` function
- `src/checker/state_checking.rs` - `check_variable_declaration`
- `src/checker/state.rs` - `get_type_of_node`
- `src/checker/state_type_analysis.rs` - `get_type_of_symbol`
- `src/checker/type_computation_complex.rs` - `get_type_of_identifier`
- `src/checker/tests/freshness_stripping_tests.rs` - Test cases

## Failing Tests

All 6 freshness stripping tests fail:
- `test_fresh_variable_can_be_reassigned_with_non_fresh_source`
- `test_freshness_preserved_for_const_with_no_type_annotation`
- `test_freshness_stripped_allows_passing_to_stricter_type`
- `test_freshness_stripped_in_function_argument`
- `test_freshness_stripped_in_let_declaration`
- `test_freshness_stripped_variable_can_be_used_as_source`

## Next Steps

This bug requires more investigation to understand the exact node caching behavior. The fix may require:
1. Understanding all the different node types involved (declaration, name, identifier expression)
2. Ensuring `get_type_of_symbol` is always called for identifier expressions
3. Making sure the widened type propagates correctly through all caches

This is a good candidate for Pro-model Gemini consultation due to the complexity.
