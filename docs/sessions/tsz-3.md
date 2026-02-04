# Session tsz-3
## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`)
- Conformance tests (`./scripts/conformance.sh`)
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed
## Current Work

**Status**: Switching focus to Binder - TS2300 (Duplicate Identifier)

**Completed This Session**:
1. **ClassDeclaration26.ts** (commit 8e21d5d71) - Fixed var constructor() pattern in class bodies
2. **abstractPropertyNegative.ts** (commit 8a034be71) - Fixed getter/setter without body check

**Completed This Session**:
1. **ClassDeclaration26.ts** (commit 8e21d5d71) - Fixed var constructor() pattern in class bodies
2. **abstractPropertyNegative.ts** (commit 8a034be71) - Fixed getter/setter without body check
3. **MODULE_DECLARATION in duplicate resolution** (commit 0a881e3cd) - Added MODULE_DECLARATION to `resolve_duplicate_decl_node` to fix namespace/class merging

**Next Focus**: TS2300 - Duplicate Identifier Errors

**Root Cause Found** (commit 0a881e3cd):
The checker's `resolve_duplicate_decl_node` function in `src/checker/symbol_resolver.rs` was missing MODULE_DECLARATION in its list of declaration node kinds. This caused namespace declarations to return None from `declaration_symbol_flags`, making them invisible to the duplicate checking logic.

**Test Case**: `cloduleTest2.ts`
- Expected: [TS2339, TS2554, TS2576]
- Actual: [TS2300, TS2300, TS2300, TS2300, TS2300, TS2554]
- tsc emits NO TS2300 for this file (namespace/class merging is allowed)
- tsz was emitting 5 TS2300 errors (one for each `declare class m3d`)

**Remaining Issue**: Binder scope merging bug.
The binder is creating ONE symbol with ALL 10 m3d declarations (5 namespaces + 5 classes from different scopes).

Expected behavior:
- T1.m3d should be a LOCAL symbol to T1 (namespace + class merged within T1)
- T2.m3d should be a LOCAL symbol to T2 (namespace + class merged within T2)
- Top-level m3d should be its own symbol
- Total: 5 separate m3d symbols

Actual behavior:
- All 10 declarations merged into 1 global m3d symbol
- Checker sees class vs class in different scopes and emits TS2300

**Investigation Status**:
- `bind_module_declaration` calls `enter_scope(ContainerKind::Module, idx)` at line 1257
- `declare_symbol` checks `self.current_scope.get(name)` which should be scope-local
- Both legacy and persistent scope systems exist and seem correct
- Need to trace actual binding flow to find where cross-scope merging happens


**Conformance Status**: 97/200 passed (48.5%)
Top error mismatches:
- TS2300: 10 missing, 1 extra ← New focus
- TS1005: 13 missing (mostly API tests with JSON parsing)
- TS2304: 6 missing, 9 extra
- TS2695: 11 missing

### Punted Items

1. **Constructor Type Bug** - The type environment maps class symbols to instance type instead of constructor type. This causes tests like `test_abstract_constructor_assignability` to fail because passing a class as a value returns the instance type (with Object.prototype properties) instead of the constructor type.
   - Attempted fix: Changed type environment to map to constructor type (didn't work)
   - Status: BLOCKED - Requires comprehensive tracing of type resolution path
   - Related to: SymbolRef resolution, type environment population

2. **Abstract Mixin Intersection** - `test_abstract_mixin_intersection_ts2339` also fails, likely related to the same type resolution issue

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

## Punted Items

1. **Constructor Type Bug** - The type environment maps class symbols to instance type instead of constructor type. This causes tests like `test_abstract_constructor_assignability` to fail because passing a class as a value returns the instance type (with Object.prototype properties) instead of the constructor type.
   - Attempted fix: Changed type environment to map to constructor type (didn't work)
   - Status: BLOCKED - Requires comprehensive tracing of type resolution path
   - Related to: SymbolRef resolution, type environment population

2. **Abstract Mixin Intersection** - `test_abstract_mixin_intersection_ts2339` also fails, likely related to the same type resolution issue
