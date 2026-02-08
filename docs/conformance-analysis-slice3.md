# Conformance Test Analysis - Slice 3

**Date**: 2026-02-08
**Slice**: 3 of 4 (offset: 6318, max: 3159)
**Baseline Pass Rate**: 56.3% (1556/2764 tests passing)

## Summary

This document summarizes analysis of conformance test failures for slice 3. The goal is to identify high-impact patterns that can improve the pass rate.

## Top Error Code Mismatches

| Error Code | Description | Missing | Extra | Priority |
|------------|-------------|---------|-------|----------|
| TS2322 | Type not assignable | 53 | 79 | HIGH |
| TS2339 | Property does not exist | 45 | 81 | HIGH |
| TS1005 | Syntax error | 35 | 37 | MEDIUM |
| TS2304 | Cannot find name | 47 | 21 | MEDIUM |
| TS2345 | Argument not assignable | 19 | 41 | MEDIUM |
| TS18048 | Possibly undefined | 27 | 15 | MEDIUM |

## Key Patterns Identified

### 1. Overly Aggressive Strict Null Checking (TS18048/18047/2532)

**Impact**: 92+ extra "possibly undefined/null" errors that TSC doesn't emit

**Root Cause**: We're emitting these errors in cases where TSC doesn't, likely due to differences in control flow analysis or type narrowing.

**Files to Investigate**:
- `crates/tsz-checker/src/type_checking_queries.rs:1023-1150` - `report_nullish_object()` function
- `crates/tsz-checker/src/state_type_analysis.rs:2182-2192` - Property access null checking

**Example Test**: `neverReturningFunctions1.ts` - expects no errors but we emit TS18048

**Complexity**: HIGH - Requires understanding when TSC's control flow analysis determines a value cannot be null/undefined

### 2. Private Name Error Codes (TS2339 vs TS18014/18016/18013)

**Impact**: Many private name tests failing because we emit generic TS2339 instead of specific private name error codes

**Pattern**: When accessing private names (#prop) incorrectly, we should emit:
- TS18014: "Property is shadowed by another private identifier"
- TS18016: "Private identifier not declared"
- TS18013: "Cannot access private identifier"

Instead, we emit: TS2339 "Property does not exist"

**Files to Investigate**:
- `crates/tsz-checker/src/state_type_analysis.rs` - Property access checking
- Need to detect when a property is a private name and emit specific error codes

**Example Tests**:
- `privateNameNestedClassFieldShadowing.ts` - expects TS18014, we emit TS2339
- `privateNameBadDeclaration.ts` - expects TS18016/TS18028, we emit TS2339

**Complexity**: MEDIUM - Requires detecting private name context and selecting correct error code

### 3. Use Before Assigned (TS2454)

**Impact**: Both false positives (emitting when we shouldn't) and missing errors (not emitting when we should)

**Pattern**: Flow analysis for variable initialization tracking
- False positive example: `controlFlowIIFE.ts` - IIFE initializes variables, we don't recognize it
- Missing example: `unusedLocalsInForInOrOf1.ts` - should detect unused variables in for-in/for-of

**Complexity**: HIGH - Requires accurate control flow graph and definite assignment analysis

## Tests Close to Passing (131 tests differ by 1-2 error codes)

These represent potential quick wins:

1. `varianceAnnotationValidation.ts` - Missing TS2636 only
2. `classAbstractFactoryFunction.ts` - Missing TS2345 only
3. `decoratorOnArrowFunction.ts` - Missing TS1005 only
4. `privateNameReadonly.ts` - Missing TS2322 only

## Recommendations

### For Immediate Impact (Low-Hanging Fruit)

1. **Fix private name error codes** - MEDIUM complexity, affects many tests
   - Detect private name access (property names starting with #)
   - Emit TS18014/18016/18013 instead of TS2339
   - Estimated impact: 20-30 tests

2. **Investigate parser/syntax issues (TS1005)** - Often simple fixes
   - 35 missing, 37 extra - suggests some cases are close
   - Check for simple syntax validation gaps
   - Estimated impact: 10-20 tests

### For Long-Term Impact (Complex but High Value)

1. **Refine strict null checking** - HIGH complexity
   - Understand when TSC's narrowing eliminates null/undefined
   - May require improvements to control flow analysis
   - Estimated impact: 50-100 tests

2. **Improve definite assignment analysis** - HIGH complexity
   - Better tracking of variable initialization through control flow
   - Handle IIFEs, for-in/for-of patterns
   - Estimated impact: 30-50 tests

## Running Tests

```bash
# Full slice
./scripts/conformance.sh run --offset 6318 --max 3159 --verbose

# Analyze patterns
./scripts/conformance.sh analyze --offset 6318 --max 3159 --top 30

# Filter by error code
./scripts/conformance.sh analyze --offset 6318 --max 3159 --error-code 2339

# Test unit tests (excluding one flaky test)
cargo nextest run -E 'not test(test_run_with_timeout_fails)'
```

## Next Steps

1. Pick one pattern to fix (recommend: private name error codes for medium complexity / good impact)
2. Write failing unit tests for the pattern
3. Implement fix
4. Run unit tests to verify no regressions
5. Re-run conformance slice to measure improvement
6. Commit and sync with main after EVERY commit
