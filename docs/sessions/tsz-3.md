# Session tsz-3
## WORK IS NEVER DONE UNTIL ALL TESTS PASS
Work is never done until all tests pass. This includes:
- Unit tests (`cargo nextest run`)
- Conformance tests (`./scripts/conformance.sh`)
- No existing `#[ignore]` tests
- No cleanup work left undone
- No large files (>3000 lines) left unaddressed
## Current Work

**Status**: Fixing parser/binder conformance test failures - focusing on TS1005 and TS1440 errors

**Context**: Per session direction, shifted focus from constructor type bug (blocked/punted) to fixing binder/parser related conformance failures.

**Current Investigation - ClassDeclaration26.ts**:
Test file expects errors we don't produce:
```typescript
class C {
    public const var export foo = 10;
    var constructor() { }
}
```

TypeScript errors:
- Line 3 col 18: TS1440 - Variable declaration not allowed at this location
- Line 5 col 5: TS1068 - Unexpected token. A constructor, method, accessor, or property was expected
- Line 5 col 20, 23: TS1005 - ',' expected, '=>' expected
- Line 6 col 1: TS1128 - Declaration or statement expected

Our errors:
- Line 3 col 12: TS1248 - A class member cannot have the 'const' keyword
- Line 3 col 22: TS1012 - Unexpected modifier
- Line 5 col 9: TS1012 - Unexpected modifier

**Root Cause**: Our parser is too permissive. It treats `public const var export foo` as a valid property declaration with modifier errors, when it should recognize this as an attempted variable declaration and fail with TS1440 at the `var` keyword.

**Code Path**:
- `parse_class_member` calls `parse_class_member_modifiers`
- Modifiers parser consumes `public`, `const` (both treated as modifiers)
- When it sees `var`, it checks if followed by `(` or line break
- Since `var` is followed by `export`, it emits "Unexpected modifier" (TS1012) but continues
- Then `export` also emits "Unexpected modifier"
- Parser continues and treats `foo` as property name

**Fix Needed**: When `var`/`let`/`const` appears in an invalid modifier context (followed by another keyword like `export`), emit TS1440 at the var position instead of continuing to parse as a property.

**Conformance Status** (max 200 tests):
- 98/200 passed (49.0%)
- Top error mismatches: TS1005 (missing=13), TS2304 (missing=6, extra=9), TS2695 (missing=10)

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
