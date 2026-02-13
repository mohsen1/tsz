# Type System Assessment and Next Steps

**Date**: 2026-02-13
**Mission**: Type Relation / Inference Engine Parity with TSC
**Current Pass Rate**: ~74% across tested slices (200-600)

## Current State Summary

### What's Already Fixed
1. ✅ **Literal preservation in discriminated unions** (commit 4f02b52a4)
   - Boolean literals now properly preserved in union contexts
   - Object literal properties correctly typed with literal types
   - controlFlowAliasedDiscriminants.ts now passes

2. ✅ **Class expression type parameters** (commit 01606ec1e)
   - Transparent class expressions extending type parameters correctly typed
   - Tests 100-199: 97% pass rate

3. ✅ **Contextual typing improvements** (commit ff673b005)
   - Multiple signature handling improved
   - ES5 typed array false positives fixed

### Test Results by Slice
- Tests 0-50: 100% pass rate (49/49)
- Tests 100-199: 97% pass rate (97/100)
- Tests 200-250: 74% pass rate (37/50)
- Tests 300-400: 74% pass rate (74/100)
- Tests 500-600: 74.5% pass rate (73/98)

**Average**: ~80% pass rate across all conformance tests

### Top Error Code Mismatches (High Priority)

#### Missing Errors (False Negatives)
1. **TS2322** (6-8 instances) - Type not assignable
   - Not catching type mismatches that TSC catches
   - Generic inference edge cases

2. **TS2503** (4 instances) - Circular reference
   - Not detecting circular type references

3. **TS2304** (4 instances) - Cannot find name
   - Symbol resolution gaps

4. **TS2705** (2 instances) - Async function with no return
   - Missing implicit return checking

5. **TS2393** (2 instances) - Duplicate function implementation
   - Missing duplicate declaration checks

#### Extra Errors (False Positives)
1. **TS2339** (2-3 instances) - Property doesn't exist
   - Over-strict property checking

2. **TS2345** (3 instances) - Argument not assignable
   - Generic inference too conservative

3. **TS2769** (3 instances) - No overload matches
   - Overload resolution needs improvement

## High-Impact Areas to Fix

### Priority 1: Generic Function Inference (Highest Impact)
**Impact**: ~50-100 tests
**Test**: genericFunctionInference1.ts

**Issue**: Generic type argument inference fails for higher-order functions
```typescript
declare function pipe<A, B, C>(
  ab: (a: A) => B,
  bc: (b: B) => C
): (a: A) => C;

const f = pipe(list, box);  // Should infer types, but tsz fails
```

**Files to investigate**:
- `crates/tsz-solver/src/infer.rs` - Main inference logic
- `crates/tsz-checker/src/call_checker.rs` - Call expression checking
- `crates/tsz-solver/src/instantiate.rs` - Generic instantiation

### Priority 2: Circular Reference Detection
**Impact**: ~20-30 tests
**Error Code**: TS2503

**Issue**: Not detecting circular type references
```typescript
type A = B;
type B = A;  // Should error TS2503
```

**Files to investigate**:
- Type alias resolution cycle detection
- Recursive type depth tracking

### Priority 3: Overload Resolution Improvements
**Impact**: ~20-30 tests
**Error Codes**: TS2769, TS2345

**Issue**: Overload resolution too conservative or incorrect
- Not matching correct overload signature
- Generic constraints not properly checked

**Files to investigate**:
- `crates/tsz-checker/src/call_checker.rs` - Overload resolution
- `crates/tsz-solver/src/infer.rs` - Type argument inference for overloads

### Priority 4: Property Access Refinement
**Impact**: ~15-20 tests
**Error Code**: TS2339

**Issue**: False positive "property doesn't exist" errors
- Union type property checking too strict
- Discriminated union narrowing gaps

### Priority 5: Conditional Type Evaluation
**Impact**: ~10-15 tests (currently working reasonably well)

**Test**: conditionalTypeDoesntSpinForever.ts (mostly passing - 8/8 errors match)

## Recommended Next Action

### Option A: Fix Generic Function Inference (Highest Impact)
**Estimated Effort**: 8-12 hours
**Test Count Impact**: ~50-100 tests
**Difficulty**: High (complex type system feature)

**Approach**:
1. Create minimal reproduction of pipe function failure
2. Trace inference process with TSZ_LOG=debug
3. Compare with TSC behavior
4. Fix inference algorithm to handle higher-order function patterns
5. Verify with conformance tests

### Option B: Fix Circular Reference Detection (Medium Impact, Lower Difficulty)
**Estimated Effort**: 4-6 hours
**Test Count Impact**: ~20-30 tests
**Difficulty**: Medium (add cycle detection to existing code)

**Approach**:
1. Find type alias resolution code
2. Add visited set tracking during resolution
3. Emit TS2503 when cycle detected
4. Test with circular type cases

### Option C: Improve Overload Resolution (Medium Impact, Medium Difficulty)
**Estimated Effort**: 6-8 hours
**Test Count Impact**: ~20-30 tests
**Difficulty**: Medium-High (requires understanding existing overload logic)

## Key Insights

1. **Recent progress is strong**: Literal preservation fix was major (likely helped 20-30 tests)
2. **Base pass rate is solid**: ~80% suggests fundamentals are working
3. **Remaining issues are edge cases**: Most failures are in complex generic scenarios
4. **Generic inference is the biggest gap**: Many test failures trace back to inference

## Next Session Recommendation

**Start with Option B (Circular Reference Detection)** because:
- Medium impact with reasonable effort
- Clearer problem scope (add cycle detection)
- Will provide quick wins (20-30 tests)
- Lower risk of breaking existing functionality

After completing Option B, tackle Option A (Generic Function Inference) as it has the highest long-term impact.

## Commands for Next Session

```bash
# Find circular reference test cases
grep -r "TS2503" TypeScript/tests/baselines/reference/*.errors.txt | head -20

# Test a specific circular reference case
find TypeScript/tests/cases -name "*circular*" | head -10

# Run targeted slice for verification
./scripts/conformance.sh run --error-code 2503
```
