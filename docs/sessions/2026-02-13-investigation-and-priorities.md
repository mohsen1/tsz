# Session Summary: Investigation and Updated Priorities

**Date**: 2026-02-13 (Continuation)
**Focus**: Investigating "close to passing" tests and reassessing priorities

## Work Completed

### 1. Investigated TS7011/TS2345 Confusion ⚠️

**Test**: `argumentsReferenceInFunction1_Js.ts`
**Issue**: tsz emits TS7011 (implicit any return) instead of TS2345 (argument type error)

**Root Cause Identified**:
- The `.apply()` method needs to use `CallableFunction` interface's generic overloads
- Current behavior suggests we're using the less specific `Function.apply` signature
- Generic overload: `apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R`
- This makes `args` a tuple type like `[f?: any]`, and `IArguments` is not assignable to tuples

**Complexity Assessment**: **Medium-High (4-6 hours, not 2-3 as estimated)**
- Requires understanding lib loading and method resolution
- Affects all `.apply()`, `.call()`, and `.bind()` usage
- Lower impact than initially thought (only 1 test, but important for correctness)

### 2. Investigated TS2322/TS2345 Confusion ⚠️

**Test**: `allowJscheckJsTypeParameterNoCrash.ts`
**Issue**: tsz emits TS2345 (argument error) instead of TS2322 (assignment error)

**Initial Assessment**: Appears to be error code selection in object literal property checking
**Complexity**: Requires minimal reproduction to properly diagnose
**Status**: Needs more investigation with proper test case

### 3. Broader Conformance Analysis ✅

**Results from first 300 tests**: **90.3% pass rate** (270/299)

**Top Issues by Frequency**:
1. **TS2769 extra (6 tests)**: "No overload matches" - We're too conservative in overload resolution
2. **TS2339 extra (4 tests)**: "Property does not exist" - We're too strict on property access
3. **TS7006 extra (4 tests)**: "Implicit any parameter" - Contextual typing gaps
4. **TS2304 missing (3 tests)**: "Cannot find name" - Missing unresolved reference detection

### 4. Updated Task Estimates

Original "quick win" estimates were too optimistic. Actual complexity:
- argumentsReferenceInFunction1_Js: 4-6 hours (not 2-3)
- allowJscheckJsTypeParameterNoCrash: 2-3 hours (needs proper reproduction)
- ambiguousGenericAssertion1: Not yet investigated

## Key Discoveries

### Discovery 1: "Close to Passing" ≠ "Easy to Fix"

Tests differing by 1-2 error codes can hide complex issues:
- Error code differences may indicate fundamental feature gaps
- Simple error code swaps are rare
- Most require understanding the underlying type system behavior

### Discovery 2: High-Impact Opportunities

**TS2769 (overload resolution)**: 6 extra emissions in 300 tests
- Indicates we're being too conservative
- Likely affects many more tests in full suite
- Could be high-impact fix if we can identify the pattern

**TS2339 (property access)**: 4 extra emissions
- We're rejecting valid property accesses
- Likely related to union type handling or narrowing

### Discovery 3: Conformance Pass Rate is Strong

- Tests 0-99: 97% pass rate
- Tests 0-299: 90.3% pass rate
- All 3547 unit tests passing
- Focus should be on high-impact fixes, not one-off issues

## Revised Priorities

### Priority 1: TS2769 Overload Resolution (NEW)
**Impact**: 6+ tests in sample (likely 20-30+ in full suite)
**Difficulty**: Medium (4-8 hours)
**Why**: Most frequent false positive, high test impact
**Approach**:
1. Analyze the 6 failing tests
2. Identify common pattern in overload rejection
3. Fix overload matching logic
4. Verify no regressions

### Priority 2: TS2339 Property Access
**Impact**: 4+ extra errors (15-20+ tests estimated)
**Difficulty**: Medium (4-6 hours)
**Why**: Second most frequent false positive
**Approach**:
1. Identify which property accesses are incorrectly rejected
2. Check if related to union types or type narrowing
3. Fix property resolution logic

### Priority 3: TS7006 Contextual Typing
**Impact**: 4+ tests (10-15 estimated)
**Difficulty**: High (8-12 hours as previously assessed)
**Why**: Known complex issue from previous analysis
**Defer**: Requires dedicated session

### Priority 4: Individual Error Code Fixes
**Impact**: 1-2 tests each
**Difficulty**: 2-6 hours each
**Why**: Lower impact, deeper investigation needed
**Include**:
- TS7011/TS2345 confusion (.apply() handling)
- TS2322/TS2345 confusion (error code selection)
- TS2304 missing errors (unresolved references)

## Session Metrics

| Metric | Value |
|--------|-------|
| Tests investigated | 3 |
| Conformance tests run | 300 |
| Pass rate discovered | 90.3% |
| High-impact issues identified | 2 (TS2769, TS2339) |
| Tasks created/updated | 2 |
| Time spent | ~3 hours |
| Code changes | 0 (investigation only) |

## Recommendations for Next Session

### Option A: Tackle TS2769 Overload Resolution (RECOMMENDED)
**Why**: Highest frequency false positive (6 tests in sample)
**Approach**:
1. Run: `./scripts/conformance.sh analyze --max=300 | grep TS2769`
2. Identify the 6 failing tests
3. Compare tsz vs TSC output for each
4. Find common pattern
5. Fix overload matching logic
6. Verify with full test suite

**Estimated Time**: 4-8 hours
**Expected Impact**: 20-30+ tests

### Option B: Focus on TS2339 Property Access
**Why**: Second highest frequency (4 extra errors)
**Similar approach to Option A**
**Estimated Time**: 4-6 hours
**Expected Impact**: 15-20+ tests

### Option C: Continue with Individual Fixes
**Why**: Lower impact but clearer scope
**Tests**: argumentsReferenceInFunction1_Js, allowJscheckJsTypeParameterNoCrash
**Estimated Time**: 2-3 hours per test
**Expected Impact**: 1-2 tests each

## Key Takeaways

### What Worked
1. ✅ Broad conformance analysis revealed high-impact patterns
2. ✅ Running 300 tests gave statistically significant error frequency data
3. ✅ Identified that focusing on error frequency is better than "close to passing"

### What Didn't Work
1. ❌ "Close to passing" tests were deceptively complex
2. ❌ Initial time estimates were too optimistic
3. ❌ Individual test fixes have low impact (1-2 tests each)

### Strategy Adjustment
- **Old strategy**: Fix "close to passing" tests one by one
- **New strategy**: Identify error patterns affecting many tests, fix root causes
- **Impact**: One fix affecting 20-30 tests >> Ten fixes affecting 1-2 tests each

## Technical Insights

### Overload Resolution (TS2769)
Being emitted 6 times when shouldn't be suggests:
- We're rejecting valid overload matches
- Likely too conservative in parameter checking
- Could be generic constraint handling
- Could be bivariance/contravariance issues

### Property Access (TS2339)
Being emitted 4 extra times suggests:
- We're not properly narrowing union types for property access
- Or we're not recognizing properties that exist on all union members
- Could be related to index signatures or mapped types

### Error Code Selection
- TS2322 vs TS2345: Assignment vs argument errors
- TS7011 vs TS2345: Return type vs argument errors
- These indicate we need clearer error emission logic
- Lower priority than fixing actual type checking

## Files for Next Session

**For TS2769 investigation**:
- `crates/tsz-checker/src/call_checker.rs` - Overload resolution
- `crates/tsz-solver/src/application.rs` - Generic application
- `crates/tsz-solver/src/infer.rs` - Type inference

**For TS2339 investigation**:
- `crates/tsz-checker/src/type_computation.rs` - Property access
- `crates/tsz-solver/src/narrowing.rs` - Type narrowing
- `crates/tsz-checker/src/control_flow_narrowing.rs` - Control flow

## Commands for Next Session

```bash
# Analyze TS2769 failures
./scripts/conformance.sh run --max=300 2>&1 | grep -B 5 "TS2769.*extra" > tmp/ts2769-analysis.txt

# Get specific failing tests
./scripts/conformance.sh analyze --max=300 --category wrong-code 2>&1 | grep -A 10 "TS2769"

# Test a specific failing case
.target/dist-fast/tsz TypeScript/tests/cases/compiler/[TEST_NAME].ts 2>&1

# Compare with TSC
cat TypeScript/tests/baselines/reference/[TEST_NAME].errors.txt
```

---

**Status**: ✅ Investigation complete - Clear path forward with revised priorities

**Key Decision**: Focus on high-frequency error patterns (TS2769, TS2339) rather than individual "close to passing" tests for maximum impact.

**Expected ROI**: Fixing TS2769 pattern could improve pass rate from 90.3% to 92-93% in one go.
