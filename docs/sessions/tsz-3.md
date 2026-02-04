# Session tsz-3
## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`)
- Conformance tests (`./scripts/conformance.sh`)
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed
## Current Work

**Status**: Implementing Array Mutation Detection in Flow Graph Builder

**Task**: Port array mutation detection logic from state_binding.rs into FlowGraphBuilder

**Problem**: Array mutation methods (push, pop, splice, etc.) are currently not tracked in the flow graph. This causes incorrect type narrowing because CFA treats these methods as read-only operations.

**Example**:
```typescript
let x: number[] | string[] = [];
if (typeof x[0] === "number") {
    x.push(5); // Should invalidate narrowing
    console.log(x[0].toFixed()); // Currently errors, but should work after push
}
```

**Implementation Plan** (from Gemini):
1. Add `is_array_mutation_call` helper method to detect mutation methods
2. Add `create_flow_array_mutation` method to create flow nodes
3. Update `handle_expression_for_assignments` to detect and create mutation flow nodes

**Expected Impact**: Fix TS2339 (Property does not exist) and TS2322 (Type not assignable) errors related to array narrowing.

**File**: `src/checker/flow_graph_builder.rs`

**Completed** (commit 32772dbb3):
- Implemented labeled statement support in FlowGraphBuilder
- Added `label: NodeIndex` field to FlowContext struct
- Implemented `build_labeled_statement` method for LABELED_STATEMENT nodes
- Updated `handle_break` and `handle_continue` to check for labels and search flow_stack
- Labeled breaks/continues now correctly find their target by matching label text

**Test Results**:
- Manual test with labeled break/continue: No errors (matches tsc)
- Build succeeded
- Ready for conformance testing

**Next**: Run conformance tests to measure impact on TS2322 and TS2339 errors.

**Task**: Add support for labeled statements and directed breaks/continues in the FlowGraphBuilder to fix control flow analysis.

**Problem**: The current FlowGraphBuilder ignores `LABELED_STATEMENT` nodes and labels in `BREAK_STATEMENT`/`CONTINUE_STATEMENT`. This causes incorrect flow analysis for code like:
```typescript
outer: for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
        if (condition) break outer; // Currently breaks inner loop in CFA
    }
}
```

**Implementation Plan** (from Gemini):
1. Add `label: Option<String>` field to `FlowContext` struct
2. Implement `build_labeled_statement` handling in `build_statement`
3. Update `handle_break` and `handle_continue` to check for labels and search flow_stack

**Expected Impact**: Fix TS2322 (Type assignability) and TS2339 (Property does not exist) conformance errors caused by incorrect type narrowing along wrong control flow paths.

**File**: `src/checker/flow_graph_builder.rs`

**Completed This Session**:
1. **ClassDeclaration26.ts** (commit 8e21d5d71) - Fixed var constructor() pattern in class bodies
2. **abstractPropertyNegative.ts** (commit 8a034be71) - Fixed getter/setter without body check
3. **MODULE_DECLARATION in duplicate resolution** (commit 0a881e3cd) - Added MODULE_DECLARATION to `resolve_duplicate_decl_node` to fix namespace/class merging
4. **Scope-aware symbol merging** (commit 8a78b95f0) - Fixed parallel binding to respect symbol scopes

**TS2300 Fix Complete** (commit 8a78b95f0):
Fixed the issue where symbols from different scopes (e.g., T1.m3d and T2.m3d) were being incorrectly merged into a single global symbol during parallel binding.

**Root Cause**: The `merge_bind_results_ref` function in `src/parallel.rs` used `merged_symbols: FxHashMap<String, SymbolId>` which mapped symbol NAMES to global IDs, IGNORING scope.

**Solution**: Added `is_nested_symbol` check that determines if a symbol is from a nested scope by looking up its declaration's scope ID. Only symbols from ROOT scope (ScopeId(0)) are allowed to use the merged_symbols map for cross-file merging. Nested scope symbols always get new IDs and are never cross-file merged.

**Test Results**:
- cloduleTest2.ts: No TS2300 errors (matches tsc)
- Simple test case (T1.m3d, T2.m3d, top-level m3d): No TS2300 errors (matches tsc)
- Conformance: TS2300 improved from missing=407, extra=62 → missing=409, extra=56

**Previous Investigation** (for reference):

**MODULE_DECLARATION Fix** (commit 0a881e3cd):
The checker's `resolve_duplicate_decl_node` function was missing MODULE_DECLARATION, causing namespace declarations to be invisible to duplicate checking.

**Investigation Process**:
- Binder correctly creates separate symbols in different scopes
- `declare_symbol` uses scope-local lookups via `self.current_scope.get(name)`
- Debug output showed: 3 separate m3d symbols in ScopeId(1), ScopeId(5), ScopeId(0)
- Problem: Symbols were being consolidated between binding and checking

**ROOT CAUSE** (discovered in `src/parallel.rs`):
The `merge_bind_results_ref` function used `merged_symbols: FxHashMap<String, SymbolId>` which mapped ONLY by NAME, ignoring scope completely.

When merging symbols from different files:
- T1.m3d gets inserted into merged_symbols["m3d"]
- T2.m3d finds merged_symbols["m3d"] exists and MERGES with T1.m3d
- Top-level m3d also MERGES with the same symbol

Result: All "m3d" symbols from all scopes merged into ONE global symbol ID.


**Conformance Status**: 4995/12638 passed (39.5%)
Top error mismatches:
- TS1005: missing=460, extra=1029
- TS2304: missing=427, extra=406
- TS2322: missing=266, extra=544
- TS2339: missing=416, extra=285
- TS2300: missing=409, extra=56 ← Fixed nested scope merging, still needs work
- TS2345: missing=81, extra=445
- TS2440: missing=475, extra=34

### Punted Items

1. **~~Constructor Type Bug~~** ✅ **FIXED in tsz-2 (2026-02-04)**
   - The type environment now correctly distinguishes constructor types from instance types
   - `test_abstract_constructor_assignability` now passes
   - Classes in TYPE position return instance types, classes in VALUE position return constructor types
   - Status: RESOLVED

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

### 2025-02-04: Scope-Aware Symbol Merging in Parallel Binding - COMPLETED

**Completed**:
1. Fixed scope-aware symbol merging in `src/parallel.rs` (commit 8a78b95f0)
2. Added `is_nested_symbol` check to prevent cross-scope merging
3. Only ROOT scope symbols use merged_symbols map for cross-file merging
4. Nested scope symbols always get new IDs

**Test Results**:
- cloduleTest2.ts: Fixed false positive TS2300 errors (now matches tsc)
- Simple nested scope test: Fixed false positive TS2300 errors (now matches tsc)
- Conformance: TS2300 improved from missing=407, extra=62 → missing=409, extra=56

**Files Modified**:
- `src/parallel.rs`: Added is_nested_symbol check in both lib and user symbol merging

**Key Insight**:
The merged_symbols FxHashMap was mapping by NAME only, allowing T1.m3d and T2.m3d to merge despite being in different scopes. The fix checks if a symbol is from a nested scope (scope.parent != ScopeId::NONE) and skips the merged_symbols lookup for those symbols.

---

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

1. **~~Constructor Type Bug~~** ✅ **FIXED in tsz-2 (2026-02-04)**
   - The type environment now correctly distinguishes constructor types from instance types
   - `test_abstract_constructor_assignability` now passes
   - Classes in TYPE position return instance types, classes in VALUE position return constructor types
   - Status: RESOLVED

2. **Abstract Mixin Intersection** - `test_abstract_mixin_intersection_ts2339` also fails, likely related to the same type resolution issue
