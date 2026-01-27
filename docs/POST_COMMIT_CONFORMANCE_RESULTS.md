# Post-Commit Conformance Test Results

**Date**: 2026-01-27
**Commit**: da0adcd8a (fix(checker): Multiple critical fixes - crashes, OOM, regressions, and error improvements)

## Test Results

**Pass Rate**: 29.9% (143/478 tests)
- Previous: 29.3% (140/478)
- **Improvement: +0.6% (+3 tests passed)**
- Time: 42.9s (11 tests/sec)

## Stability Improvements 🎉

### Dramatic Reduction in Failures
| Category | Before | After | Change |
|----------|--------|-------|--------|
| Worker Crashes | 113 | 0 | ✅ -113 |
| Test Crashes | 11 | 1 | ✅ -10 |
| OOMs | 10 | 0 | ✅ -10 |
| Timeouts | 52 | 0 | ✅ -52 |

**Total Stability Improvement**: -185 failures (99.5% reduction)

### Remaining Crash
**1 Test Still Crashing**: `compiler/allowJsCrossMonorepoPackage.ts`
- Error: `TypeError: Cannot read properties of undefined (reading 'flags')`
- Location: Cross-package symbol resolution
- Same root cause as before - symbol IDs from other packages not validated

## Error Changes

### Extra Errors (errors we emit but shouldn't)

| Error Code | Before | After | Change | Status |
|------------|--------|-------|--------|--------|
| TS2339 | 449x | 440x | -9 | ✅ Improving |
| TS2749 | 261x | 196x | -65 | ✅ Improving |
| TS2322 | 176x | 168x | -8 | ✅ Improving |
| TS7010 | 176x | 163x | -13 | ✅ Improving |
| TS2571 | - | 136x | +136 | ⚠️ New top category |
| TS2507 | - | 120x | +120 | ⚠️ New top category |
| TS2304 | 73x | 93x | +20 | ⚠️ Regressed |
| TS2345 | - | 73x | +73 | ⚠️ New top category |

### Missing Errors (errors we should emit but don't)

| Error Code | Before | After | Change | Status |
|------------|--------|-------|--------|--------|
| TS2339 | 189x | 32x | -157 | ✅ Major improvement |
| TS2318 | - | 226x | +226 | ℹ️ Expected (--noLib) |
| TS2304 | - | 60x | +60 | ℹ️ New top missing |
| TS18048 | - | 59x | +59 | ℹ️ Non-nullish operator |
| TS2488 | - | 53x | +53 | ⚠️ Iterator protocol |

## Analysis

### What Worked Well ✅

1. **Cycle Detection**: Eliminated stack overflow OOMs
2. **Defensive Programming**: Reduced crashes from 124 to 1
3. **Union Property Logic**: TS2339 both missing (-157) and extra (-9) improved
4. **Async Function Handling**: TS7010 reduced by 13 errors
5. **Symbol Flag Propagation**: TS2749 reduced by 65 errors

### What Needs Work ⚠️

1. **Cross-Package Symbol Access**: The 1 remaining crash
2. **TS2304 Caching**: May have over-aggressive caching (extra errors increased)
3. **TS2571**: Emerged as top category (object literal implicit this)
4. **TS2507**: Emerged as top category (union constructor calls)
5. **TS18048**: Non-nullish operator not implemented

## Next Priority Fixes

### 1. Fix the Remaining Crash (HIGHEST PRIORITY)
**Test**: `compiler/allowJsCrossMonorepoPackage.ts`
**Error**: `Cannot read properties of undefined (reading 'flags')`
**Location**: Cross-package symbol resolution
**Strategy**:
- The issue is that when importing symbols from other packages, the SymbolId may reference symbols not in the current binder's symbol table
- Need to use `GlobalSymbolId` and resolve to the correct package's binder
- Already partially fixed in some places, but this test case hits a different code path

### 2. Investigate TS2304 Regression
**Issue**: Extra errors increased from 73x to 93x
**Hypothesis**: Caching in `get_type_from_type_node` may be caching too aggressively
**Strategy**:
- Review caching logic in type resolution
- Ensure cache is keyed by all relevant context
- May need to invalidate cache in certain scenarios

### 3. Investigate TS2571 Emergence
**Issue**: 136x errors (was not in top list before)
**Meaning**: Object literal implicitly has 'any' type for 'this'
**Strategy**:
- Add test case to debug suite
- Investigate object literal type inference
- Check if this is a real issue or just more tests running

### 4. Investigate TS2507 Emergence
**Issue**: 120x errors (was not in top list before)
**Meaning**: Constructor calls on union types
**Strategy**:
- May be related to our union constructor fix
- Review test cases to see if errors are correct or spurious
- Could be that previous crashes prevented these from being counted

### 5. Fix TS2488 Iterator Protocol
**Issue**: 53x missing errors
**Status**: Partially implemented in previous session
**Strategy**:
- Review optional Symbol.iterator check implementation
- May need additional cases for spread operator, for-of loops

## Conclusion

The fixes in commit da0adcd8a were highly successful:
- **Stability**: 99.5% reduction in crashes/OOMs/timeouts
- **Correctness**: +3 tests passing, -157 missing TS2339 errors
- **Error Quality**: Significant reductions in TS2339, TS2749, TS7010

The remaining work focuses on:
1. The last crash (cross-package symbols)
2. Investigating newly-emerged error categories
3. Continuing systematic error reduction

**Estimated Path to 100%**:
- Need +335 more tests to pass (currently 143/478)
- If each commit fixes ~3-10 tests: ~35-100 commits needed
- Focus on high-impact errors (TS2339, TS2749, TS2322) for fastest progress
