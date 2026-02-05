# Session TSZ-2: Circular Type Parameter Inference

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix 5 failing circular `extends` tests in solver::infer

## Problem Statement

The current implementation fails to handle circular type parameter constraints (e.g., `T extends U, U extends T`) correctly. The solver uses simple iteration limits instead of proper coinductive (Greatest Fixed Point) resolution, causing inference to fail or produce incorrect results.

## Goal

Fix all 5 circular extends tests in `src/solver/infer.rs`:
1. `test_circular_extends_chain_with_endpoint_bound`
2. `test_circular_extends_three_way_with_one_lower_bound`
3. `test_circular_extends_with_literal_types`
4. `test_circular_extends_with_concrete_upper_and_lower`
5. `test_circular_extends_conflicting_lower_bounds`

## Gemini Pro Analysis

**Root Cause**: The `strengthen_constraints` function (lines 1208-1230 in `src/solver/infer.rs`) uses a simple iteration limit (`MAX_CONSTRAINT_ITERATIONS`) but doesn't implement true coinductive resolution for mutually dependent type parameters.

When cycles occur (e.g., `T extends U, U extends T`), the inference engine often defaults to `unknown` or triggers an `OccursCheck` error prematurely.

## Suggested Approach

Based on Gemini Pro's recommendation:

1. **Modify `strengthen_constraints`**: Implement dependency-graph-based resolution
   - Identify strongly connected components (SCCs) of type parameters
   - Parameters within the same SCC should be unified or resolved together

2. **Refine `occurs_in`**: Distinguish between:
   - Illegal recursion (type containing itself in a way that can't be lazily expanded)
   - Legal recursive constraints (F-bounded polymorphism like `T extends Comparable<T>`)

3. **Update `compute_constraint_result`**: When a cycle is detected, find the "least restrictive" type that satisfies the cycle by looking at the `extends` constraints

## Test Cases

### Test 1: Chain with Endpoint
```typescript
// T extends U, U extends V, V extends number
// Should resolve to number
```

### Test 2: Three-Way with Lower Bound
```typescript
// T extends U, U extends V, T extends string
// Should incorporate string constraint
```

### Test 3: Literal Types
```typescript
// Circular extends with literal type constraints
// Should preserve literal information
```

### Test 4: Concrete Upper and Lower
```typescript
// Both upper and lower bounds in a cycle
// Should find intersection or compatible type
```

### Test 5: Conflicting Lower Bounds
```typescript
// Multiple lower bounds in a cycle
// Should find best common type
```

## Code Locations

- `src/solver/infer.rs:330-374`: `expand_cyclic_upper_bound`
- `src/solver/infer.rs:1208-1230`: `strengthen_constraints`
- `src/solver/infer.rs:1232-1282`: `propagate_lower_bound` / `propagate_upper_bound`

## Potential Pitfalls

1. **Infinite Recursion**: Be extremely careful with `propagate_upper_bound`. If `T extends U` and `U extends T`, a naive propagator will bounce between them forever.

2. **Over-widening**: If circularity detection is too aggressive, you might resolve everything to `any` or `unknown`, violating the "Match tsc" goal.

## Dependencies

- Session tsz-1: Core type relations (must coordinate to avoid conflicts)
- Session tsz-3: Narrowing (no overlap - different domain)

## Next Steps

1. Run the 5 failing tests to see current error messages
2. Add tracing to understand the current inference flow
3. Ask Gemini Pro: "What's the right algorithm for coinductive type parameter resolution?"
4. Implement SCC-based constraint resolution
5. Test and iterate

## Notes

This is a deeper architectural challenge that requires understanding:
- Coinductive type systems (Greatest Fixed Point)
- Dependency graphs and Strongly Connected Components
- F-bounded polymorphism
