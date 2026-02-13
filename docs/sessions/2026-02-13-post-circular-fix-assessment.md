# Post-Circular Fix Assessment & Next Priorities

**Date**: 2026-02-13
**Status**: Assessment Complete
**Current Overall Pass Rate**: ~87% average across tested slices

## Current Pass Rates by Slice

| Slice | Pass Rate | Passing/Total | Notes |
|-------|-----------|---------------|-------|
| 0-99 | 96.0% | 95/99 | Excellent |
| 100-199 | 96.0% | 96/100 | Excellent |
| 200-299 | 75.0% | 75/100 | Needs attention |
| 300-399 | 80.0% | 80/100 | Good |

**Average**: 87% across all tested slices

## Top Error Code Patterns

### Slice 200-299 (Lowest Pass Rate)

**False Positives** (Extra Errors):
1. **TS2769** (No overload matches): 6 instances
   - Overload resolution too conservative
   - Generic type inference not working for overloaded functions

2. **TS7006** (Implicit any parameter): 4 instances
   - Contextual typing gaps
   - Not inferring parameter types from function type constraints

3. **TS2339** (Property doesn't exist): 4 instances
   - Union type property access too strict
   - Discriminated union narrowing incomplete

**False Negatives** (Missing Errors):
1. **TS2740** (Missing properties): 3 instances
   - Not catching all required property violations
   - Possibly related to generic constraints

### Slice 300-399

**False Negatives** (Missing Errors):
1. **TS2322** (Type not assignable): 6 instances
   - Generic inference misses
   - Subtype relationships not properly checked

**False Positives** (Extra Errors):
1. **TS2345** (Argument not assignable): 3 instances
   - Generic type argument inference too strict

## Detailed Analysis: TS7006 Contextual Typing Issue

### Test Case: contextuallyTypedParametersWithInitializers1.ts

**Expected (TSC)**: 4 errors (TS7006) on lines 27, 43
**Actual (tsz)**: ~11 errors (TS7006, TS7011, TS7019) on lines 10, 11, 16, 17, 27, 28, 43, 44, 50, 55, 78

**Problem**: We're not inferring parameter types from contextual types when:
- Parameter has a default value: `(x = 1) => 0`
- Parameter is destructured: `({ foo = 42 }) => foo`
- Parameter is optional: `(x?) => 0`
- Parameter is rest: `(...x) => 0`

### Example That Should Work

```typescript
declare function id5<T extends (x?: number) => any>(input: T): T;

// TSC: Infers x has type number from constraint
// tsz: Reports TS7006 because we don't use contextual type
const f25 = id5(function (foo = 42) { return foo });
```

**Root Cause**: In `call_checker.rs` or contextual typing code, we're not propagating the constraint type parameter into the parameter type inference.

**Files to Investigate**:
- `crates/tsz-checker/src/call_checker.rs` - Generic call handling
- `crates/tsz-solver/src/contextual.rs` - Contextual type propagation
- `crates/tsz-solver/src/infer.rs` - Type inference from constraints

## Detailed Analysis: Generic Function Inference

### Test Case: genericFunctionInference1.ts

**Expected (TSC)**: 3 errors
**Actual (tsz)**: ~30-40 errors (124 lines of output)

**Problem**: Higher-order generic function inference completely broken.

### Example That Fails

```typescript
declare function pipe<A extends any[], B, C>(
  ab: (...args: A) => B,
  bc: (b: B) => C
): (...args: A) => C;

declare function list<T>(a: T): T[];
declare function box<V>(x: V): { value: V };

const f01 = pipe(list, box);  // Should infer composition type
```

**Current Errors**:
- TS2769: No overload matches
- Can't infer type arguments A, B, C
- Falls back to treating arguments as incompatible

**Root Cause**: Multi-parameter generic inference with constraints not working.

**Complexity**: HIGH - This is the core generic inference engine

## Priority Recommendations

### Priority 1: TS7006 Contextual Parameter Typing (Medium Impact, Medium Difficulty)
**Impact**: ~10-15 tests
**Estimated Time**: 4-6 hours
**Difficulty**: Medium

**Why prioritize**:
- Clear problem scope
- Single feature (contextual parameter typing)
- Affects common pattern (callbacks with default params)

**Implementation Approach**:
1. Find where function expression parameters are type-checked
2. When parameter has default value/destructuring/optional, check for contextual type
3. If contextual type is a function type with matching signature, use its parameter types
4. Add unit tests for each case (default, destructured, optional, rest)

### Priority 2: TS2740 Missing Property Checks (Low Impact, Low Difficulty)
**Impact**: ~5-10 tests
**Estimated Time**: 2-3 hours
**Difficulty**: Low-Medium

**Why prioritize**:
- Missing errors are easier to add than fixing false positives
- Clear spec: check all required properties are present
- Less risk of breaking existing tests

### Priority 3: Generic Function Inference (High Impact, High Difficulty)
**Impact**: 50-100+ tests
**Estimated Time**: 12-20 hours
**Difficulty**: Very High

**Why defer**:
- Extremely complex
- Requires deep understanding of inference algorithm
- High risk of regressions
- Better tackled after simpler fixes build confidence

### Priority 4: TS2769 Overload Resolution (Medium Impact, High Difficulty)
**Impact**: ~20-30 tests
**Estimated Time**: 8-12 hours
**Difficulty**: High

**Why defer**:
- Complex - involves overload resolution + generic inference
- Related to Priority 3
- Better to fix contextual typing first

## Recommended Next Session Plan

### Option A: Fix TS7006 Contextual Parameter Typing (Recommended)
**Approach**:
1. Create minimal test cases for each scenario
2. Trace current behavior with TSZ_LOG
3. Find parameter type checking code
4. Add contextual type lookup when parameter needs inference
5. Verify with unit tests and conformance tests

**Expected Outcome**: Reduce TS7006 false positives by ~10-15 tests

### Option B: Add TS2740 Missing Property Checks
**Approach**:
1. Find where object type assignability is checked
2. Add check for required properties
3. Emit TS2740 when required property is missing
4. Test with conformance suite

**Expected Outcome**: Catch ~5-10 missing errors

## Code Locations for Priority 1 (TS7006 Fix)

### Parameter Type Checking
```
crates/tsz-checker/src/function_type.rs
  - check_function_expression
  - get_parameter_type
```

### Contextual Type Propagation
```
crates/tsz-checker/src/dispatch.rs
  - Function expression dispatch
  - Arrow function dispatch
```

### Contextual Type Extraction
```
crates/tsz-solver/src/contextual.rs
  - get_parameter_type()
  - ContextualTypeContext
```

## Test Commands

```bash
# Test contextual parameter typing
.target/dist-fast/tsz TypeScript/tests/cases/compiler/contextuallyTypedParametersWithInitializers1.ts

# Expected: Only 4 errors on lines 27, 43
# Actual: 11 errors

# Compare with TSC
cat TypeScript/tests/baselines/reference/contextuallyTypedParametersWithInitializers1.errors.txt

# Test generic inference
.target/dist-fast/tsz TypeScript/tests/cases/compiler/genericFunctionInference1.ts

# Expected: 3 errors
# Actual: ~40 errors

# Run conformance on problematic slice
./scripts/conformance.sh run --max=100 --offset=200
```

## Summary

Current state after TS2456 circular reference fix:
- ✅ Overall pass rate: **87%** (very good)
- ✅ Slices 0-199: **96%** (excellent)
- ⚠️ Slice 200-299: **75%** (needs work)
- ✅ Slice 300-399: **80%** (good)

**Main gaps**:
1. Contextual parameter typing (TS7006 false positives)
2. Generic function inference (many errors)
3. Overload resolution (too conservative)

**Recommended next fix**: TS7006 contextual parameter typing
- Clear scope
- Medium impact
- Builds toward generic inference
- 4-6 hours estimated

---

**Status**: Ready for TS7006 fix in next session
