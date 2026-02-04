# Session tsz-1: Incremental Conformance Improvements

**Started**: 2026-02-04 (Fourth iteration)
**Goal**: Incrementally improve conformance by fixing simple, achievable issues

## Previous Achievements in tsz-1
1. ✅ Parser fixes (6 fixes, 38% → 50% conformance)
2. ✅ TS2318 core global type checking fix
3. ✅ Duplicate getter/setter detection fix

## Current Approach
Focus on high-value, low-risk fixes that:
1. Can be completed in under 1 hour each
2. Have clear test cases
3. Improve conformance measurably
4. Don't require architectural changes

## Potential Tasks (Priority Order)

### Task 1: Fix More TS1005 Errors
- **Status**: Partially complete (6 variants fixed)
- **Remaining**: ~12 TS1005 variants still missing
- **Files**: `src/parser/`
- **Timebox**: 15 minutes per variant
- **Impact**: Direct conformance improvement

### Task 2: Simple Type Checking Fixes
- Review 52 failing tests for simple patterns
- Focus on diagnostic emission issues (missing/wrong errors)
- Avoid complex type resolution problems

### Task 3: Test Infrastructure
- Improve test error messages
- Add missing test coverage
- Better test organization

## Success Metrics
- Number of tests fixed
- Conformance percentage improvement
- No regressions introduced
- All work committed and documented

## Status: READY FOR NEXT TASK
Analyzing failing tests to identify next achievable fix.
