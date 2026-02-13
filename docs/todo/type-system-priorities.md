# Type System Improvement Priorities

**Date**: 2026-02-13
**Status**: Investigation complete, ready for implementation

## Current State

- **Unit Tests**: ✅ 2394/2394 passing (100%)
- **Conformance**: ~97% pass rate overall
- **Focus Area**: Core type system inference and overload resolution

## High-Impact Issues Identified

### 1. Generic Function Overload Resolution ⚠️ HIGH PRIORITY

**Issue**: Overload resolution fails for generic functions with multiple signatures

**Test Case**: `genericFunctionInference1.ts`
- TSC: 1 error (expected)
- tsz: ~7 errors (incorrect)

**Minimal Reproduction**: `tmp/pipe-inference.ts`
```typescript
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function pipe<A extends any[], B, C>(ab: (...args: A) => B, bc: (b: B) => C): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const f01 = pipe(list, box);  // ERROR: No overload matches (should work)
```

**Symptoms**:
- Error says "Expected 1 arguments" and "Expected 3 arguments" but never "Expected 2 arguments"
- The 2-argument overload isn't being tried or is failing silently
- Affects many higher-order function inference scenarios

**Root Cause Found** ✓:
Through detailed tracing, discovered that:
1. The 2-argument overload IS being tried correctly
2. The first argument PASSES the check (Callable → Function works)
3. The second argument FAILS with: `Callable(10331) <: Function(3943)` rejected

Created minimal reproduction showing the actual bug:
```typescript
declare function box<V>(x: V): { value: V };
const f: (x: number) => { value: number } = box;  // ERROR in tsz, OK in TSC
```

**The Real Bug**: **Generic Callables cannot be assigned to concrete Function types**

In TypeScript, a generic function like `<V>(x: V) => {value: V}` SHOULD be assignable to a concrete function type like `(x: number) => {value: number}` because the generic can be instantiated. Currently tsz rejects this assignment.

**Location**: `crates/tsz-solver/src/subtype_rules/functions.rs:680` - `check_callable_to_function_subtype`

**Fix Strategy**: When checking `Callable <: Function`:
1. If Callable has type parameters and Function doesn't
2. Try to instantiate the Callable's call signatures to match the Function's parameter types
3. Check if the instantiated signature is compatible

**Impact**: Affects all generic higher-order functions (50+ conformance tests)

---

### 2. Mapped Type Inference ⚠️ BLOCKED - Architectural

**Issue**: Homomorphic mapped types in generic function parameters don't infer correctly

**Test Case**: `mappedTypeRecursiveInference.ts`

**Root Cause**: Type definitions are registered AFTER functions using them are type-checked

**Status**: **BLOCKED** - Requires architectural changes to type registration

**Documentation**:
- `docs/issues/mapped-type-inference.md`
- `docs/sessions/2026-02-13-mapped-type-inference-wip.md`

**Recommendation**: Defer until architecture refactor for two-phase type checking

**Impact**: Affects ~20 tests, users can work around with explicit annotations

---

### 3. Conditional Type Evaluation ~ MINOR DIFFERENCES

**Test Case**: `conditionalTypeDoesntSpinForever.ts`
- TSC: 8 errors
- tsz: 8 errors (slightly different locations)

**Status**: Very close, just minor line number differences

**Priority**: LOW (already mostly correct)

---

### 4. Contextual Typing ✅ WORKING

**Test Case**: `contextualTypingOfLambdaWithMultipleSignatures2.ts`
- Status: ✅ PASSING (matches TSC exactly)

---

## Recommended Action Plan

### Immediate (This Session)
1. **Fix Generic Overload Resolution**
   - Add detailed tracing to understand why 2-arg overload isn't matched
   - Check if the issue is in argument count bounds or type inference
   - Fix and verify against `genericFunctionInference1.ts`
   - Run full conformance to check for regressions

### Short Term (Next Session)
2. **Conditional Type Edge Cases**
   - Review minor differences in `conditionalTypeDoesntSpinForever.ts`
   - Ensure error locations are precise

### Long Term (Future Architecture Work)
3. **Two-Phase Type Checking**
   - Design type registration system
   - Implement deferred constraint generation
   - Enable mapped type inference

## Success Metrics

- **Target**: 98%+ conformance pass rate
- **Focus**: Fix issues that unblock multiple tests
- **Quality**: Maintain 100% unit test pass rate

## Code Locations

**Overload Resolution**:
- `crates/tsz-checker/src/call_checker.rs:281` - resolve_overloaded_call_with_signatures
- `crates/tsz-solver/src/operations.rs:576` - resolve_function_call
- `crates/tsz-solver/src/operations.rs:611` - resolve_generic_call

**Generic Inference**:
- `crates/tsz-solver/src/infer.rs` - Type inference logic
- `crates/tsz-solver/src/operations.rs:1900-2760` - constrain_types

**Testing**:
- Run specific test: `.target/dist-fast/tsz TypeScript/tests/cases/compiler/TEST.ts`
- Compare with TSC: `cat TypeScript/tests/baselines/reference/TEST.errors.txt`
- Unit tests: `cargo nextest run`
- Conformance: `./scripts/conformance.sh run`
