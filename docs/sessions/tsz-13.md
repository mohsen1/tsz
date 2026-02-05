# Session TSZ-13: Type Inference for Function Calls

**Started**: 2026-02-05
**Status**: 🔄 PENDING
**Focus**: Implement type argument inference from function call value arguments

## Summary

This session implements **Type Argument Inference** to enable TypeScript-style generic function calls without manual type specification.

### Problem Statement

**Current Issue**: Generic functions require manual type instantiation
```typescript
function identity<T>(x: T): T {
    return x;
}

// Currently requires explicit type:
const result = identity<string>("hello");

// Should work but might not:
const result = identity("hello"); // Should infer T = string
```

**Root Cause**:
- When checking `identity("hello")`, the type parameter `T` is not bound
- The Solver needs to infer `T` from the value argument type `string`
- Missing logic to extract type arguments from argument types during call checking

### Goal

Match `tsc` behavior for type inference in function calls:
1. Infer type parameters from argument types
2. Handle multiple type parameters with constraints
3. Support partial inference (some params explicit, some inferred)
4. Handle union/intersection types in inference

## Success Criteria

### Test Case 1: Simple Inference
```typescript
function identity<T>(x: T): T {
    return x;
}
const result = identity("hello");
// Expected: result is string (T inferred as string)
```

### Test Case 2: Multiple Type Parameters
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}
const result = map([1, 2, 3], x => x.toString());
// Expected: T = number, U = string
```

### Test Case 3: Constrained Type Parameters
```typescript
function log<T extends { id: string }>(obj: T): void {
    console.log(obj.id);
}
log({ id: "abc", name: "test" });
// Expected: T inferred as { id: string; name: string }
```

## Implementation Plan

### Phase 1: Basic Inference Infrastructure
**File**: `src/solver/infer.rs`

**Tasks**:
1. Create `infer_type_arguments_from_call` function
2. Extract type parameter bindings from value argument types
3. Use constraint solving to infer type parameter values

### Phase 2: Integration with Call Checking
**File**: `src/checker/expr.rs` or `src/checker/type_checking.rs`

**Tasks**:
1. Trigger inference before checking function body
2. Apply inferred type arguments to function signature
3. Fall back to explicit type arguments if provided

### Phase 3: Edge Cases
**Tasks**:
1. Handle partial type argument specification
2. Support union types in inference
3. Handle recursive type inference
4. Error reporting for failed inference

## Dependencies

- **tsz-6**: Member Resolution (COMPLETE) - provides property access on generics
- **tsz-5**: Multi-Pass Inference (COMPLETE) - provides contextual typing infrastructure

## Related Sessions

- **tsz-5**: Multi-Pass Generic Inference (COMPLETE) - focused on contextual typing
- **tsz-2**: Coinductive Subtyping (ACTIVE) - provides subtype checking for constraints

## Implementation Notes

### Key Functions to Investigate

1. **Solver API**: `src/solver/infer.rs`
   - Look for existing inference functions
   - Check `infer_type_from` or similar
   - Understand Union-Find or constraint-based inference

2. **Checker Integration**: `src/checker/type_checking.rs`
   - Find `check_call_expression`
   - Understand how type arguments are currently handled
   - Determine where to insert inference logic

3. **Instantiation**: `src/solver/instantiate.rs`
   - Use `instantiate_type_with_infer` for proper infer var handling
   - Apply inferred type arguments to function signatures

## MANDATORY Gemini Workflow

Per AGENTS.md, **MUST ask Gemini TWO questions**:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --include=src/solver/infer.rs --include=src/checker/type_checking.rs "
I'm starting tsz-13: Type Inference for Function Calls.

Goal: Make identity('hello') infer T = string without explicit type annotation.

My planned approach:
1) Find the function that checks call expressions in the Checker
2) Add logic to infer type parameters from value argument types
3) Use the existing inference infrastructure in src/solver/infer.rs
4) Apply inferred types to the function signature before checking the call

Questions:
1) What is the exact function in src/checker that handles call expressions?
2) Does src/solver/infer.rs already have type argument inference logic?
3) What's the TypeScript algorithm for inferring type parameters from arguments?
4) Are there edge cases I need to handle (unions, intersections, constrained types)?

Please provide: file paths, function names, and implementation guidance.
"
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/infer.rs --include=src/checker/type_checking.rs "
I implemented type inference for function calls.

Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is this logic correct for TypeScript?
2) Did I miss any edge cases?
3) Are there type system bugs?

Be specific if it's wrong - tell me exactly what to fix.
"
```

## Session History

Created 2026-02-05 following completion of tsz-6 (Member Resolution on Generic and Placeholder Types).
