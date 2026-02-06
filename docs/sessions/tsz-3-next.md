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

### Next Steps

1. Understand how `infer` types are currently represented
2. Check if conditional type evaluation handles `infer` correctly
3. Test with a simpler `infer` case to isolate the issue
4. Ask Gemini for approach validation before implementing fixes

### Test Cases

**Simple `infer` test** (for isolation):
```typescript
type ExtractParam<T> = T extends (x: infer P) => void ? P : never;
type T1 = ExtractParam<(x: string) => void>; // should be string
type T2 = ExtractParam<number>; // should be never
```

### MANDATORY Gemini Workflow

Per AGENTS.md, before implementing:

**Question 1 (Approach)**:
```bash
./scripts/ask-gemini.mjs --include=src/solver/evaluate_rules/conditional.rs --include=src/solver/types.rs "I'm debugging conditional type inference with 'infer' keywords. The issue is that 'infer TParams' and 'infer TReturn' aren't being resolved in conditional types.

Test case:
type InferThunkActionCreatorType<T> = T extends (...args: infer TParams) => infer TReturn ? TParams : T;

The error shows 'Cannot find name TParams' and 'Cannot find name TReturn'.

Please explain:
1. How are 'infer' types represented in tsz (TypeKey variant)?
2. Which function handles pattern matching for conditional types with 'infer'?
3. What's the correct approach for capturing inferred types and substituting them?"
```

**Question 2 (Review)**: After implementation, submit for review.

### Progress

- 2026-02-06: Initial Application Type Expansion investigation - found working correctly
- 2026-02-06: Identified actual issue is with conditional type `infer` inference
- 2026-02-06: Created this session file to track the new task
