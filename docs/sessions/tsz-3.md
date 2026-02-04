# Session tsz-3
## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`)
- Conformance tests (`./scripts/conformance.sh`)
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed
## Current Work

**Status**: Heritage clause symbol resolution fixed, but test fails due to separate bug

**Fix Applied**:
Updated `resolve_heritage_symbol` in `src/checker/class_inheritance.rs` to use `self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx)` instead of `self.ctx.binder.get_node_symbol(expr_idx)`.

**Why This Fix Was Needed**:
- `node_symbols` HashMap only contains mappings for declaration sites (where symbols are declared)
- Heritage clause identifiers are *references*, not declarations
- `resolve_identifier` walks the scope chain to find the symbol by name

**Remaining Issue**:
The test `test_abstract_constructor_assignability` still fails because when `Animal` is used as a value (e.g., `createAnimal(Animal)`), its type incorrectly includes Object.prototype properties like `isPrototypeOf`, `propertyIsEnumerable`, etc.

Error message:
```
Type '{ isPrototypeOf: { (v: Object): boolean }; propertyIsEnumerable: { (v: PropertyKey): boolean }; constructor: Function; speak: { (): void }; ... }' is not assignable to type 'Animal'.
```

TypeScript shows no errors for this test, confirming it should work.

**Investigation**:
Added debug logging and traced the issue:
1. `get_type_of_node` → `get_type_of_identifier` → `get_type_of_symbol` → `get_class_constructor_type`
2. For Animal class, we return `TypeId(144)` as the constructor type
3. But `TypeId(144)` contains instance properties like `speak` and Object.prototype methods

**Root Cause Found**:
The constructor type `TypeId(144)` has **0 properties**, which is correct! Animal has no static members.

However, the type environment (src/checker/state_type_analysis.rs:852-858) maps class symbols to their **instance type**, not their constructor type.

```rust
if let Some((instance_type, class_params)) = class_env_entry {
    env.insert(SymbolRef(sym_id.0), instance_type);  // BUG: Should be constructor type!
}
```

This creates an inconsistency:
- `get_type_of_symbol(Animal)` → returns constructor type (TypeId(144)) with 0 properties ✓
- `type_env.get(Animal)` → returns instance type with Object.prototype properties ✗

When the assignability check looks up the type of `Animal` (the value), it might be using the type environment which returns the instance type instead of the constructor type.

**Attempted Fix**:
Changed type environment to map class symbols to constructor type instead of instance type (state_type_analysis.rs:857). This didn't fix the test, suggesting the issue is elsewhere.

**Reverted** the change since it didn't work and might have other impacts.

**Status**: BLOCKED - Need deeper investigation into type resolution path

The constructor type is correctly created with 0 properties, and `get_type_of_symbol` returns it. However, the error message shows instance properties, suggesting a different resolution path is being used.

**Blocked By**: Complex interaction between type environment, symbol resolution, and type formatting that requires comprehensive tracing to understand.

**Failing Tests**:
1. `test_abstract_constructor_assignability` - TS2322 error: Type for `Animal` includes Object.prototype properties
2. `test_abstract_mixin_intersection_ts2339` - TS2339 errors for missing properties

**Last completed**: Const Type Parameters (TS 5.0) Implementation (2025-02-04)

**Next Step**: Need to trace through type resolution for `typeof Animal` and understand why instance prototype properties are being included in the type.

**Last completed**: Const Type Parameters (TS 5.0) Implementation (2025-02-04)

### Other Sessions Active
- **tsz-1**: Conformance test analysis (error mismatches: TS2705, TS1109, etc.)
- **tsz-2**: Module resolution errors (TS2307, TS2318, TS2664)
- **tsz-4**: Test fixes, compilation error cleanup

### Potential Next Tasks

Based on the Gemini analysis, the next priority areas for complex types are:

1. **Variance Calculation** - Full structural variance calculation for generic types
2. **Instantiation Caching** - Performance optimization for repeated generic instantiations
3. **Readonly Inference for Const Type Params** - Add readonly modifiers to object/array types inferred with const type parameters (future enhancement)

### Next Priority Areas (from Gemini analysis)

According to the analysis of the codebase, the next priorities for complex types are:

1. **Variance Calculation** - Full structural variance calculation for generic types
2. **Instantiation Caching** - Performance optimization for repeated generic instantiations
3. **Readonly Inference for Const Type Params** - Add readonly modifiers to object/array types inferred with const type parameters (future enhancement)

---

## History (Last 20)

### 2025-02-04: Const Type Parameters (TS 5.0) - COMPLETED

**Completed**:
1. Updated `InferenceContext` in `src/solver/infer.rs` to track `is_const` flag for type parameters
2. Changed `type_params` from `Vec<(Atom, InferenceVar)>` to `Vec<(Atom, InferenceVar, bool)>`
3. Updated `fresh_type_param` and `register_type_param` to accept `is_const` flag
4. Added `is_var_const` helper to check if an inference variable is const
5. Updated `resolve_from_candidates` to skip widening when `is_const` is true
6. Updated all callers of `fresh_type_param` and `register_type_param` across the codebase
7. Fixed all test files to pass the `is_const` flag
8. Added 3 new tests for const type parameter behavior

**Test Results**: All 545 inference tests pass

**Files Modified**:
- `src/solver/infer.rs`: Core const type parameter logic (is_var_const, updated resolve_from_candidates)
- `src/solver/operations.rs`: Pass `tp.is_const` when creating type parameter placeholders
- `src/solver/tests/*.rs`: Updated all test calls to pass `false` for non-const type params

**Notes**:
- The implementation correctly preserves literal types for const type parameters
- Single literal candidates are preserved even for non-const type params (matches TypeScript behavior)
- Multiple different literals widen to primitive types (matches TypeScript behavior)
- Readonly inference for const type parameters is a future enhancement

### 2025-02-03: Const Type Parameters (TS 5.0) - Partial Implementation

**Completed**:
1. Added `is_const: bool` field to `TypeParamInfo` struct in `src/solver/types.rs`
2. Added `has_const_modifier` function in `src/solver/lower.rs` to detect const keyword
3. Updated `lower_type_parameter` to set `is_const` flag based on modifiers

---

### 2025-02-03: Tail-Recursion Elimination for Conditional Types

**Implemented**: Tail-recursion elimination in `src/solver/evaluate_rules/conditional.rs`

- Modified `evaluate_conditional` to use a loop structure instead of direct recursion
- Added `MAX_TAIL_RECURSION_DEPTH` constant (1000) separate from `MAX_EVALUATE_DEPTH` (50)
- When a conditional branch evaluates to another `ConditionalType`, the loop continues instead of recursing
- This allows patterns like `type Loop<T> = T extends [infer A, ...infer R] ? Loop<R> : never` to work with up to 1000 iterations

**Key Changes**:
1. Wrapped evaluation logic in a `loop` with mutable `current_cond` state
2. After evaluating true/false branches, check if result is a `ConditionalType`
3. If yes and within `MAX_TAIL_RECURSION_DEPTH`, update `current_cond` and `continue`
4. Otherwise, return the result

**Files Modified**:
- `src/solver/evaluate_rules/conditional.rs`: Core TRE implementation
- `src/solver/tests/evaluate_tests.rs`: Added `test_tail_recursive_conditional`

**Notes**:
- The implementation runs without depth limit crashes
- Test needs further debugging to verify correct unwinding behavior
- One pre-existing test failure unrelated to this change: `test_generic_parameter_without_constraint_fallback_to_unknown`

---

## Punted Todos

*No punted items*
