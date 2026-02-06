# Session tsz-3: Conditional Type Inference with `infer` Keywords

**Started**: 2026-02-06
**Status**: ✅ COMPLETE
**Predecessor**: tsz-3-checker-conformance

## Summary

Successfully fixed conditional type inference with `infer` keywords by enhancing the `collect_infer_type_parameters_inner` function to recursively check for `InferType` nodes in nested type structures.

## Problem

The error `"Cannot find name 'R'"` was occurring for conditional types:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
type T1 = GetReturnType<() => string>; // Error: TS2304: Cannot find name 'R'
```

## Root Cause

The `collect_infer_type_parameters_inner` function in `src/checker/type_checking_queries.rs` (lines 1151-1188) only checked for `InferType` nodes in:
- Direct `InferType` nodes
- Type references with type arguments
- Union/Intersection types

It did **NOT** check inside function types, arrays, tuples, and other nested type structures where `infer` keywords can appear.

## Solution

Extended `collect_infer_type_parameters_inner` to recursively check for `InferType` nodes in:
1. Function/Constructor types (parameters and return type)
2. Array types (element type)
3. Tuple types (all elements)
4. Type literals (all members)
5. Type operators (keyof, readonly, unique)
6. Indexed access types (object and index)
7. Mapped types (type parameter constraint, type template)
8. Conditional types (all branches)
9. Template literal types (spans)
10. Parenthesized, optional, rest types
11. Named tuple members
12. Parameters (type annotations)
13. Type Parameters (constraint and default) - added per Gemini code review

## Commits

1. **`2c238b893`**: feat(checker): fix infer type collection in nested types
   - Main implementation of recursive infer type collection

2. **`4eab170d1`**: fix(checker): add TYPE_PARAMETER handling for infer collection
   - Added TYPE_PARAMETER case per Gemini Pro code review
   - Handles `<T extends infer U>` and `<T = infer U>` patterns

3. **`760008ff2`**: docs(ts3-next): mark session complete
   - Session documentation marked complete

## Test Results

**Before Fix**:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
// Error: TS2304: Cannot find name 'R'
```

**After Fix**:
```typescript
type GetReturnType<T> = T extends () => infer R ? R : never;
type T1 = GetReturnType<() => string>; // ✅ Works - T1 is string

type ExtractParam<T> = T extends (x: infer P) => void ? P : never;
type T2 = ExtractParam<(x: string) => void>; // ✅ Works - T2 is string
```

**Conformance Impact**:
- Redux test: "Cannot find name TParams/TReturn" errors eliminated
- Dozens of conformance tests using conditional type inference now pass

## Gemini Consultation

**Question 1** (Approach Validation):
- Asked about root cause of "Cannot find name" error
- Gemini identified the issue in `collect_infer_type_parameters_inner`

**Question 2** (Implementation Review):
- Submitted implementation for review
- Gemini identified missing `TYPE_PARAMETER` case for constraints/defaults
- Suggested refactoring default case for clarity

## Related Sessions

- **tsz-3-checker-conformance**: In operator narrowing fix (completed)
- **tsz-3-status**: Status summary document
- **tsz-3-next**: This session's working file (renamed to `-complete.md`)

## Next Steps

See `tsz-3-status.md` for current conformance baseline (68/100 passing).
Recommended next task: Run conformance suite to identify remaining high-priority failures.
