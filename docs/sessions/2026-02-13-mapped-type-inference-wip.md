# Mapped Type Inference - Work in Progress

**Date**: 2026-02-13
**Status**: BLOCKED - Requires architectural changes (see below)

## Summary

Investigated and partially fixed mapped type inference for generic functions with mapped type parameters like `foo<T>(arg: {[K in keyof T]: T[K]}): T`.

## Achievements

### ✅ Verified Working
1. **Contextual Typing** (contextualTypingOfLambdaWithMultipleSignatures2.ts) - PASSING
2. **Unit Tests** - All 2394 passing
3. **Conditional Types** - 98% working (8 errors match TSC, minor differences only)

### 🔧 Mapped Type Inference - Partial Implementation
Added handling for Application and Lazy types in constraint generation:
- `crates/tsz-solver/src/operations.rs`: +37 lines
- `crates/tsz-solver/src/infer.rs`: +60 lines  

**Changes**:
1. Added Application/Lazy evaluation in `constrain_types_impl` before Mapped type check
2. Added `infer_mapped_type` method to detect homomorphic mapped types
3. Added logic to infer type parameters from source when target is homomorphic mapped type

## Root Cause Analysis

### Problem
When calling `id<T>(arg: Identity<T>): T` with `Point`:
1. `Identity<T>` = `{[K in keyof T]: T[K]}`
2. Need to infer `T` from constraint: `Point <: Identity<T>`
3. Currently returns `unknown` instead of `Point`

### Investigation Path
1. **Constraint Generation Flow**:
   - `Point <: Identity<__infer_N>` where `__infer_N` is fresh inference variable
   - `Identity<__infer_N>` is a TypeApplication
   - Needs evaluation before we can see the underlying mapped type

2. **Type Evaluation**:
   - `evaluate_type(Identity<__infer_N>)` →  
   - Instantiates base to: `{[K in keyof __infer_N]: __infer_N[K]}`
   - Tries to evaluate mapped type
   - `keyof __infer_N` can't be resolved → deferred
   - Returns mapped type unchanged

3. **Expected Behavior**:
   - Should detect homomorphic mapped type pattern
   - Infer `T = Point` by "reversing" the mapping
   - Currently: not working despite implementation

## Current Issue

The fix is implemented but not working. The mapped type case in `operations.rs` (line 2069+) is not being hit during constraint generation, even though:
- Application types ARE being evaluated
- The evaluation should produce a deferred mapped type  
- The mapped type case should match

**Hypothesis**: The instantiation process may be creating a different structure than expected, or the var_map lookup is failing because the TypeParameter in `keyof T` after instantiation isn't the same TypeId that's in var_map.

## Next Steps

1. **Debug with detailed tracing**:
   - Add extensive logging to track TypeIds through instantiation
   - Verify var_map contents during constraint generation
   - Check if mapped type case is actually being reached

2. **Verify var_map structure**:
   - After instantiation, what TypeId represents the inference variable?
   - Is it the same TypeId used in `keyof __infer`?
   - Does `var_map.get(&keyof_inner)` find the right entry?

3. **Alternative approach if current fails**:
   - Instead of checking var_map in operations.rs, could check in infer.rs
   - The `infer_mapped_type` method might need different logic
   - May need to track inference variables differently

## Code Locations

- **Constraint generation**: `crates/tsz-solver/src/operations.rs:2062-2112`
- **Inference logic**: `crates/tsz-solver/src/infer.rs:1107-1466`
- **Type evaluation**: `crates/tsz-solver/src/evaluate.rs:208-392`
- **Mapped type evaluation**: `crates/tsz-solver/src/evaluate_rules/mapped.rs:144-250`

## Test Cases

```typescript
// tmp/test-mapped-simple.ts
interface Point { x: number; y: number }
declare let p: Point;

type Identity<T> = { [K in keyof T]: T[K] }
declare function id<T>(arg: Identity<T>): T;

const result = id(p);
// TSC: result is Point  
// tsz: result is unknown ❌
```

## Documentation Created

- `docs/issues/mapped-type-inference.md` - Complete analysis
- Current session notes

## Warning

Uncommitted changes exist. They compile but don't fix the issue yet. Need more debugging before committing.

## Updated Investigation (Later Same Day)

### Multiple Fix Attempts - All Failed

**Attempt 1**: Evaluate Applications in `constrain_types`
- Added `(_, Application)` case to evaluate target before constraining
- **Failed**: `evaluate_type()` returns unevaluated Application

**Attempt 2**: Manual instantiation  
- Tried calling `resolve_lazy()` and `get_lazy_type_params()` directly
- **Failed**: Both return `None` during constraint generation

### Confirmed Root Cause

Used extensive tracing to confirm the architectural issue:
```
During constraint generation:
  DefId(21749): has_type_params=false, has_resolved=false

Later in execution:
  DefId(21749): has_type_params=true, type_params_count=1, has_resolved=true, resolved_key=Some(Mapped(...))
```

**Conclusion**: Type definitions are registered AFTER functions using them are type-checked.

### Why This is Hard

Cannot fix without architectural changes because:
1. Type aliases need to be fully registered before constraint generation
2. Current architecture interleaves type checking and type registration
3. Application evaluator fundamentally depends on resolver having complete info
4. All workarounds hit the same limitation: type info not available yet

### Recommendation

File this as a known limitation requiring architectural work. The issue affects:
- Homomorphic mapped types in generic function parameters  
- Any pattern where type inference relies on expanding type alias applications

Not blocking for initial release - users can work around with explicit annotations.
