# Generic Function Inference - Pipe Pattern Issue

## Status
**DIAGNOSED** - Root cause identified, solution approach documented

## Problem Summary
When passing generic functions as arguments to other generic functions (the "pipe" pattern), tsz fails to properly infer type parameters, resulting in `unknown` types instead of preserving the generic relationships.

## Failing Test
`TypeScript/tests/cases/compiler/genericFunctionInference1.ts`
- Expected: 1 error (TS2345 on line 138)
- Actual: ~50 errors (TS2769, TS2322, TS2339, etc.)

## Minimal Reproduction

```typescript
declare function pipe<A extends any[], B, C>(
  ab: (...args: A) => B,
  bc: (b: B) => C
): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

// TSC: Works correctly, infers f01: <T>(a: T) => { value: T[] }
// tsz: FAILS with TS2769 - infers B = unknown
const f01 = pipe(list, box);
```

## Root Cause Analysis

### Current Behavior (crates/tsz-solver/src/operations.rs:2271-2445)

When constraining a generic source function against a target parameter type:

1. Source: `list<T>` = `<T>(a: T) => T[]`
2. Target: `ab: (...args: A) => B`
3. The solver creates fresh inference variables for `list`'s type parameter: `__infer_src_1` for `T`
4. Instantiates: `(a: __infer_src_1) => __infer_src_1[]`
5. Constrains: `__infer_src_1[] <: B`
6. **BUG**: `__infer_src_1` has no constraints (no calls to `list` provide concrete types)
7. When resolved: `__infer_src_1` → `unknown`, therefore `B` → `unknown[]`

### Expected Behavior (TypeScript)

TypeScript preserves the polymorphic nature:
1. Recognizes `list` is a generic function passed as an argument (not called)
2. Infers that `B` should remain generic, related to the parameter of the returned function
3. Result: `f01: <T>(a: T) => { value: T[] }` where the generic parameter is preserved

This is **higher-rank polymorphism** - the ability to pass polymorphic functions as first-class values while preserving their generic nature.

## Key Code Locations

- **Generic source function handling**: `crates/tsz-solver/src/operations.rs:2271-2445`
  - Function: `constrain_types_impl` - case `(TypeKey::Function, TypeKey::Function)` with generic source
- **Type inference resolution**: `crates/tsz-solver/src/infer.rs`
- **Call expression checking**: `crates/tsz-checker/src/type_computation_complex.rs:986`

## Solution Approach

### Option 1: Defer Instantiation of Generic Arguments
Instead of immediately instantiating generic functions with fresh variables when passed as arguments:
1. Detect when a function argument's target parameter type has inference variables
2. Keep the source function generic (don't instantiate its type parameters yet)
3. Constrain the generic function's "shape" against the target
4. Let the type parameter relationships flow through naturally

### Option 2: Bi-directional Constraint Propagation
After constraining with combined var_map (outer + source vars):
1. Resolve the source inference variables
2. Substitute resolved source vars back into constrained types
3. Add the fully resolved types as candidates for outer variables

### Option 3: Special Handling for Function-Typed Arguments
Add a check in `constrain_types` for when:
- Source is a generic function
- Target contains inference variables
- No concrete arguments are being passed to the source function

In this case, treat it as a higher-order polymorphic case and preserve the generic signature.

## Verification

Created minimal test case in `tmp/pipe_simple.ts`:
```typescript
declare function pipe<A extends any[], B, C>(
  ab: (...args: A) => B,
  bc: (b: B) => C
): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const f01 = pipe(list, box);
```

Running tsz produces:
```
error TS2769: No overload matches this call.
  Argument of type '{ (x: V): { value: V } }' is not assignable to parameter of type '(b: unknown) => unknown'.
```

The `(b: unknown) => unknown` confirms that `B` is being inferred as `unknown`.

Running TSC produces: **No errors** ✓

## Next Steps

1. Study TypeScript's `checker.ts` implementation of `inferTypes` and `getInferenceMapper`
2. Look for TypeScript's handling of "higher-order type parameter inference"
3. Implement one of the solution approaches above
4. Add regression tests for pipe patterns:
   - `pipe(f)` - single generic function
   - `pipe(f, g)` - composition of two generic functions
   - `pipe(f, g, h)` - composition of three generic functions
   - With type annotations: `const f: <T>(x: T) => T[] = pipe(list)`

## Related Issues

- This affects ANY pattern where generic functions are passed as arguments
- Higher-order utility functions like `compose`, `pipe`, `map`, `filter` when used with generic callbacks
- React HOCs (Higher-Order Components) that wrap generic components

## Impact

- Blocks ~100+ conformance tests that use function composition patterns
- Core TypeScript feature for functional programming styles
- High priority for TypeScript compatibility

## References

- [TypeScript Handbook - Generics](https://www.typescriptlang.org/docs/handbook/2/generics.html)
- [Higher-Rank Polymorphism](https://en.wikipedia.org/wiki/Parametric_polymorphism#Higher-rank_polymorphism)
- TypeScript Issue #30215: "Generic function inference through function composition"
