# Tuple Inference from Function Arguments - Debugging Session

**Date**: 2026-02-13
**Status**: Work in Progress - Bounds Violation Issue
**Related**: tuple-rest-parameter-implementation.md

## Problem

Extending tuple rest parameter unpacking to work for generic inference from function arguments. When calling:

```typescript
declare function test<A extends any[]>(fn: (...args: A) => void): A;
const result = test((x: string) => {});  // Should infer A = [string]
```

Currently infers `A = any[]` instead of `A = [string]`.

## Root Cause

Function arguments are contextually sensitive, so they're processed in Round 2 of `resolve_generic_call_inner`, not Round 1 where I initially added tuple inference logic.

## Implementation Attempt

### Changes Made

1. **Round 2 tuple inference** (line 888-940 in operations.rs):
   - Added logic after contextual argument constraint collection
   - Detects when target parameter is a function with rest param type parameter
   - Extracts source function's parameters and creates tuple
   - Adds tuple as candidate for the rest param type parameter

2. **Fixed constraint direction** in constrain_types_impl:
   - Changed from `add_upper_bound` to `add_candidate`
   - Parameters are contravariant, so tuple should be a candidate (lower bound)

### Current Issue: Bounds Violation

The tracing shows:
```
Resolving type parameter, type_param_name="A", var=InferenceVar(0),
constraints=Some(ConstraintSet {
  lower_bounds: [TypeId(59299)],  // The tuple [string]!
  upper_bounds: [TypeId(158), TypeId(10)]  // any[], STRING
})
Constraint resolution failed, using fallback,
error=BoundsViolation { var: InferenceVar(0), lower: TypeId(59299), upper: TypeId(10) }
```

**Analysis**:
- Lower bound: `TypeId(59299)` = tuple `[string]` ✅ CORRECT
- Upper bound 1: `TypeId(158)` = `any[]` ✅ CORRECT (from constraint)
- Upper bound 2: `TypeId(10)` = `STRING` ❌ WRONG!

The bounds violation happens because `[string] <: string` is false.

### Mystery: Where is STRING Coming From?

The upper bound `STRING` is being added somewhere, but not from my code. Possible sources:

1. **rest_tuple_inference_target**: This function (line 1465) handles rest parameter tuple inference for array types. It might be incorrectly extracting element types and adding them as constraints.

2. **Existing constraint collection**: There might be other code paths that add element type constraints for array type parameters.

3. **Multiple overload attempts**: The tracing shows multiple resolution attempts, suggesting overload resolution. Each attempt might be adding conflicting constraints.

## Debugging Attempts

### Tracing Upper Bounds

Added instrumentation to `add_upper_bound` in infer.rs, but the STRING upper bound doesn't appear in the trace, suggesting it's added during type parameter initialization or through a different code path.

### Key Observation

The bounds violation happens BEFORE my Round 2 tuple inference:
```
Resolving type parameter A:
  lower_bounds: [TypeId(59299)]    # My tuple [string]
  upper_bounds: [TypeId(158), TypeId(10)]  # any[], STRING
```

The STRING appears to be present from the start, not added by my code.

### Hypothesis

When registering type parameter `A extends any[]`, something is extracting the element type and adding it as an upper bound. Possible locations:
1. Type parameter constraint processing in `register_type_param`
2. Array element type extraction during constraint initialization
3. `rest_tuple_inference_target` running before my code and adding constraints

## Next Steps

1. **Find type parameter registration**:
   - Check where `A extends any[]` constraint is processed
   - Look for element type extraction in constraint setup
   - Search for calls to `add_upper_bound` during type param init

2. **Consider alternative approach**:
   - Disable element type extraction for tuple-inferred parameters
   - Add a flag to type parameters to skip array element inference
   - Process tuple inference BEFORE constraint setup

3. **Quick Win Alternative**:
   - Move on to other conformance issues and return to this later
   - The foundation is solid; this is a constraint ordering/initialization issue
   - Focus on issues with clearer solutions

## Code Locations

- **Round 2 tuple inference**: `crates/tsz-solver/src/operations.rs:888-940`
- **Constraint resolution**: `crates/tsz-solver/src/operations.rs:922-954`
- **rest_tuple_inference_target**: `crates/tsz-solver/src/operations.rs:1465-1550`

## Test Cases

- `tmp/simple_inference.ts` - Basic single/multi-parameter inference
- `tmp/pipe_simple.ts` - Original pipe pattern motivation
- `tmp/rest_param_test.ts` - Direct type compatibility (works!)

## Partial Success

✅ **Function-to-function type comparison works**: Direct assignments like `const x: F2 = f1` work correctly

❌ **Generic call inference doesn't work**: Function calls like `wrapper((x: string) => {})` still infer `any[]`

The foundation is in place; the issue is with how constraints are being combined during inference resolution.
