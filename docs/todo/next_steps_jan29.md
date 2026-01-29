# Next Steps - Conformance 70%+ Goal

## Current Status (Jan 29, 2026 - PM)

**Pass Rate**: 41.4% (207/500)
**Starting Point**: 39.2% (197/500)
**Improvement**: +2.2 percentage points (+10 tests)
**Goal**: 70% (350/500)
**Gap**: +28.6 percentage points (+143 tests needed)

## Completed Work (Session 1)

### ✅ TS2335 Super Keyword Fix - 88% Reduction
- Commit: `aa9b5838b`
- Impact: 144x → 17x errors
- Fix: Walk parent chain to find enclosing class

### ✅ TS2705 Async Function Fix - Limited Impact
- Commit: `f39711b66`, `55b393d44`
- Impact: 73x → 71x errors (only 2x improvement)
- Fix: Bypass recursion guard for primitives, syntactic fallback
- Verified: Complex scenarios (generics, arrays, interfaces) working correctly
- Note: Remaining 71x are edge cases

## Top Missing Errors (500-test sample)

| Error Code | Count | Description | Complexity |
|------------|-------|-------------|------------|
| TS2705 | 71x | Async function must return Promise | ⚠️ Partially fixed, edge cases |
| TS2584 | 47x | Unknown error code | ❓ Needs investigation |
| TS2804 | 32x | Unknown error code | ❓ Needs investigation |
| TS2446 | 28x | Class visibility mismatch | ❓ Needs investigation |
| TS2488 | 23x | Symbol.iterator required | 🔧 Well-defined feature |
| TS2445 | 26x | Property is protected | ✅ Implemented, edge cases |
| TS2300 | 67x | Duplicate identifier | ⚠️ Complex architectural issue |
| TS2339 | 15x | Property access | ✅ Previously fixed |

## Investigation Results

### TS2705 (Async Function Return Type)
**Status**: ✅ Core functionality working
- Primitive types: ✅ Working
- Generic types: ✅ Working
- Array types: ✅ Working
- Object types: ✅ Working
- Union types: ✅ Working
- Interface types: ✅ Working

**Remaining Issues**: 71x edge cases
- Possibly: Conditional types
- Possibly: Generic constraints
- Possibly: Promise-wrapped types

### TS2445 (Protected Property Access)
**Status**: ✅ Implemented in `src/checker/property_checker.rs`
**Call Sites**:
- `function_type.rs:576` - Function calls
- `type_computation.rs:931` - String property access
- `type_computation.rs:941` - Numeric property access

**Remaining Issues**: 26x edge cases
- Possibly: Super property access
- Possibly: Protected static members
- Possibly: Derived class access patterns

## Next High-Impact Tasks

### Priority 1: Quick Wins (< 2 hours)

1. **Investigate TS2584 and TS2804**
   - Unknown error codes
   - May not be implemented
   - Quick investigation needed

2. **Fix TS2488 (Symbol.iterator)**
   - Well-defined feature
   - Used in for-of loops and spread operator
   - Check if iterator protocol is implemented

3. **Debug TS2445 Edge Cases**
   - Check specific failing tests
   - May be simple bug in accessibility logic

### Priority 2: Medium Complexity (2-4 hours)

4. **Improve TS2705 Coverage**
   - Investigate the 71x remaining failures
   - Check if they're test infrastructure issues
   - May need additional type resolution improvements

5. **Investigate TS2446**
   - Class visibility mismatch
   - Related to inheritance
   - May require understanding TypeScript's rules

### Priority 3: Architectural Work (4-8 hours)

6. **TS2300 Duplicate Identifier**
   - Complex issue with lib merge patterns
   - Requires careful handling of lib vs user code
   - Documented as complex architectural task

## Investigation Approach

For each error code:
1. Find the diagnostic message in `src/checker/types/diagnostics.rs`
2. Search for where it's emitted in the codebase
3. Check if it's implemented
4. If not implemented, add implementation
5. If implemented, debug why it's not firing
6. Use `npx tsc` to verify expected behavior
7. Create test cases
8. Fix the issue
9. Verify with conformance tests

## Test Categories with Lowest Pass Rates

| Category | Pass Rate | Failing Tests | Potential Impact |
|----------|-----------|---------------|------------------|
| superCalls | 10% | 9/10 | High (TS2335 already fixed) |
| inheritanceAndOverriding | 20% | 16/20 | High (class features) |
| classAbstractKeyword | 24.1% | 22/29 | Medium (abstract classes) |
| constructorParameters | 25% | 9/12 | Medium (parameter properties) |
| classHeritageSpecification | 29.4% | 12/17 | Medium (extends/implements) |
| accessibility | 42.9% | 12/21 | Low (TS2445 partially working) |

## Recommended Next Session

1. Start with **TS2584** and **TS2804** investigation (unknown codes, quick wins)
2. Move to **TS2488** (Symbol.iterator, well-defined)
3. Debug **TS2445** edge cases (already implemented)
4. If time permits, investigate **TS2446** (class visibility)

**Expected Impact**: +10-15 tests (2-3 percentage points)
**Time Investment**: 2-4 hours

## Files to Modify

### Likely Targets:
- `src/checker/type_checking.rs` - Main type checking logic
- `src/checker/property_checker.rs` - Property access
- `src/checker/class_checker.rs` - Class-related checks
- `src/checker/for_of_checker.rs` - For-of loop checking (TS2488)
- `src/checker/spread_checker.rs` - Spread operator checking (TS2488)

### Documentation:
- Update `work_summary_jan29.md` with progress
- Update this file with completed work

## Commit Strategy

After each fix:
1. Run `./conformance/run.sh --max=500` to verify
2. Update work summary
3. Commit with descriptive message
4. Push to remote

## Success Metrics

- Short-term (next session): 45%+ (225/500)
- Mid-term (week): 60%+ (300/500)
- Goal: 70%+ (350/500)
