# Session tsz-3: Conditional Type Inference with `infer` Keywords

**Started**: 2026-02-06
**Status**: Active - Investigation Phase
**Predecessor**: tsz-3-checker-conformance (completed in operator narrowing and TS2339 fixes)

## Initial Task: Application Type Expansion

### Investigation Results (2026-02-06)

**Finding**: Basic Application Type Expansion is already working correctly.

Tested with:
```typescript
type Reducer<S, A> = (state: S | undefined, action: A) => S;
const reducer: Reducer<State, Action> = (state, action) => state || { count: 0 };
```
Result: ✅ Passes without errors

**Conclusion**: The `evaluate_application` function in `src/solver/evaluate.rs` (lines 410-492) is already functioning correctly for simple generic type applications.

## Actual Issue: Conditional Type Inference

The actual failing Redux test (`reactReduxLikeDeferredInferenceAllowsAssignment.ts`) has errors:

```
error TS2304: Cannot find name 'TParams'.
error TS2304: Cannot find name 'TReturn'.
error TS2345: Argument of type '...' is not assignable to parameter of type 'error<error<...>>'
```

The problematic code:
```typescript
type InferThunkActionCreatorType<
  TActionCreator extends (...args: any[]) => any
> = TActionCreator extends (
  ...args: infer TParams  // <-- infer TParams
) => (...args: any[]) => infer TReturn  // <-- infer TReturn
  ? (...args: TParams) => TReturn
  : TActionCreator;
```

### Problem

When TypeScript evaluates a conditional type with `infer` keywords:
1. It checks if the `check_type` matches the `extends_type` pattern
2. If it matches, the `infer` clauses capture types from the pattern
3. These captured types are substituted into the `true_type` branch

In the Redux test:
- `TParams` and `TReturn` should be inferred from the function signature
- But they're not being resolved, showing "Cannot find name" errors

### Files to Investigate

#### `src/solver/evaluate_rules/conditional.rs`
- **`evaluate_conditional`**: Main conditional type evaluation logic
- **`infer_type_variables`**: Should handle `infer` type variable extraction
- **`check_extends_with_inference`**: Pattern matching for `extends` clause with `infer`

#### `src/solver/types.rs`
- **`InferVar`** or **`Infer`** type: Represents an `infer T` type variable
- How are `infer` types represented in the type system?

### Investigation Progress (2026-02-06 Continued)

**Discovery**: `match_infer_function_pattern` already exists!
- Located in `src/solver/evaluate_rules/infer_pattern.rs` (line 1129)
- Comprehensive implementation for function type inference
- Handles: parameter inference, return type inference, callables, unions

**Test Results**:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
type T1 = GetReturnType<() => string>; // Error: Cannot find name 'R'
```

**Key Finding**: The error "Cannot find name 'R'" suggests the issue is NOT in the Solver's pattern matching logic, but rather in:
1. **Type lowering**: `InferType` AST nodes may not be converted to `TypeKey::Infer` correctly
2. **Symbol scoping**: The Binder may not be declaring `infer` symbols in the correct scope
3. **Error reporting**: The error message might be coming from name resolution, not type inference

**Files Investigated**:
- ✅ `src/solver/evaluate_rules/infer_pattern.rs` - Has `match_infer_function_pattern` (line 1129)
- ✅ `src/solver/evaluate_rules/conditional.rs` - Calls `match_infer_pattern` at line 205
- ✅ `src/parser/node.rs` - Has `InferTypeData` struct and `infer_types` vector
- ❓ Type lowering from AST `InferType` to `TypeKey::Infer` - NOT FOUND YET

### Next Steps

1. Find where `InferType` AST nodes are lowered to `TypeKey::Infer`
2. Check if `TypeKey::Infer` is being created correctly
3. Verify that `match_infer_pattern` is actually being called
4. Ask Gemini about type lowering for `InferType`

### Progress

- 2026-02-06: Initial Application Type Expansion investigation - found working correctly
- 2026-02-06: Identified actual issue is with conditional type `infer` inference
- 2026-02-06: Discovered `match_infer_function_pattern` already exists
- 2026-02-06: **ROOT CAUSE FOUND**: `collect_infer_type_parameters_inner` in `type_checking_queries.rs` did not check for InferType nodes inside nested type structures
- 2026-02-06: **FIX IMPLEMENTED**: Added recursive checking for InferType in function types, arrays, tuples, type literals, type operators, indexed access, mapped types, conditional types, template literals, parenthesized/optional/rest types, named tuple members, parameters, and type parameters
- 2026-02-06: **VERIFIED**: Test cases passing, Redux test "Cannot find name" errors eliminated
- 2026-02-06: **CODE REVIEW**: Gemini reviewed implementation, suggested adding TYPE_PARAMETER handling for constraints/defaults
- 2026-02-06: **COMPLETE**: All fixes implemented and committed

### Commits

- `2c238b893`: feat(checker): fix infer type collection in nested types
- `4eab170d1`: fix(checker): add TYPE_PARAMETER handling for infer collection

### Test Results

**Before Fix**:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
// Error: TS2304: Cannot find name 'R'
```

**After Fix**:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
type T1 = GetReturnType<() => string>; // ✅ Works - T1 is string
```
