# Type Inference Gap: Rest Parameter Tuples

**Date**: 2026-02-13
**Priority**: HIGH (blocks ~200+ generic inference tests)
**Status**: Root cause identified

---

## Problem Statement

When inferring type parameters from functions with rest parameters, we fail to capture the full parameter tuple.

### Test Cases

#### Test 1: Simple rest parameter wrapping
```typescript
declare function wrap<A extends any[], R>(fn: (...args: A) => R): (...args: A) => R;
declare function add(a: number, b: number): number;

const wrapped = wrap(add);
```

**Expected**: `A = [number, number]`, `R = number`
**Actual**: `A = number` (wrong!), causes `number & any[]` error

#### Test 2: Generic function with rest parameters
```typescript
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function list<T>(a: T): T[];

const f = pipe(list);
```

**Expected**: `A = [T]`, `B = T[]`, return type `<T>(a: T) => T[]`
**Actual**: Return type `unknown`

#### Test 3: Works without rest parameters
```typescript
declare function wrap<T>(fn: () => T): () => T;
declare function getString(): string;

const f = wrap(getString);
```

**Result**: ✅ Works correctly! Infers `T = string`

---

## Root Cause

**Location**: `crates/tsz-solver/src/infer.rs` - type parameter inference from function types

**Issue**: When matching against a rest parameter constraint `A extends any[]`, we need to:
1. Recognize the target is a rest parameter tuple
2. Capture ALL parameters from the source function as a tuple type
3. Infer `A = [Param1Type, Param2Type, ...]`

**Current behavior**: We infer individual parameter types instead of collecting them into a tuple.

---

## Impact

This issue blocks:
- **Generic inference tests**: ~200+ tests
- **Higher-order function patterns**: pipe, compose, curry, etc.
- **Library typing**: RxJS, lodash, ramda, etc.
- **Utility types**: Parameters<T>, ReturnType<T>, etc.

**Test files affected**:
- `genericFunctionInference1.ts` (50+ errors)
- Any test using higher-order functions with rest parameters
- Utility type tests

---

## Solution Approach

### Required Changes

**1. Detect Rest Parameter Patterns**
   - When inferring against `(...args: A) => R` where `A extends any[]`
   - Identify that `A` should be inferred as a parameter tuple

**2. Collect Parameters as Tuple**
   - When source is `(p1: T1, p2: T2) => R`
   - Infer `A = [T1, T2]` not individual parameters

**3. Handle Generic Sources**
   - When source is `<T>(a: T) => T[]`
   - Infer `A = [T]` preserving type parameters

### Implementation Files

**Primary**: `crates/tsz-solver/src/infer.rs`
- `infer_type_arguments_from_types`
- Function type matching logic
- Rest parameter recognition

**Secondary**: `crates/tsz-solver/src/instantiate.rs`
- Type parameter substitution
- Tuple type handling

**Testing**: `crates/tsz-solver/src/tests/infer_tests.rs`
- Add tests for rest parameter inference
- Verify tuple capture works

---

## Complexity Estimate

**Effort**: 3-5 sessions

**Why complex**:
1. Need to detect rest parameter patterns reliably
2. Tuple type construction from parameters
3. Interaction with existing inference logic
4. Many edge cases (optional params, rest + regular, etc.)
5. High regression risk (inference is core)

**Alternatives**:
- None - this is fundamental to TypeScript's type system
- Must be fixed for library typing to work

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_infer_rest_parameter_tuple() {
    // wrap<A extends any[], R>(fn: (...args: A) => R)
    // with add(a: number, b: number): number
    // should infer A = [number, number]
}

#[test]
fn test_infer_rest_parameter_generic() {
    // pipe<A extends any[], B>(ab: (...args: A) => B)
    // with list<T>(a: T): T[]
    // should infer A = [T], B = T[]
}
```

### Conformance Tests
- Run `genericFunctionInference1.ts` after fix
- Expected: 1 error (the intentional one on line 138)
- Current: 50+ errors

---

## Current Status

**Investigation**: Complete ✅
**Root Cause**: Identified ✅
**Fix**: Not yet implemented ❌

**Unit Tests**: 2394/2394 passing
**Conformance**: ~62% overall (generic tests failing)

---

## Next Steps

1. Study `crates/tsz-solver/src/infer.rs` function type inference
2. Identify where parameter types are collected
3. Add rest parameter detection logic
4. Modify to collect parameters as tuple when rest param detected
5. Add unit tests for rest parameter cases
6. Run full test suite to check for regressions
7. Test with `genericFunctionInference1.ts`

---

## References

- Test: `TypeScript/tests/cases/compiler/genericFunctionInference1.ts`
- Baseline: `TypeScript/tests/baselines/reference/genericFunctionInference1.errors.txt`
- Code: `crates/tsz-solver/src/infer.rs`
- TypeScript handbook: Generics, Rest Parameters, Tuple Types

---

**Conclusion**: Rest parameter tuple inference is critical for generic higher-order functions. This is a high-priority fix that unblocks hundreds of tests. The issue is well-isolated and has a clear implementation path.
