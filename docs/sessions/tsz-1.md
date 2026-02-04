# Session tsz-1: Simple Diagnostic Fixes (Continued)

**Started**: 2026-02-04 (Seventh iteration)
**Goal**: Fix simple diagnostic emission issues with clear test cases

## Previous Achievements (from history)
1. ✅ Parser fixes (6 TS1005 variants)
2. ✅ TS2318 core global type checking
3. ✅ Duplicate getter/setter detection
4. ✅ Switch statement flow analysis (TS2564)
5. ✅ Lib contexts fallback for global symbols
6. ⏸️ Interface property access (documented as complex)
7. ⏸️ Discriminant narrowing (documented as complex)

## Completed Work

### Test: test_duplicate_class_members

**Issue**: Test expected 2 TS2300 errors but only 1 was being emitted

**Investigation**:
- Traced duplicate detection logic in `src/checker/state_checking_members.rs`
- Found conflicting test expectations:
  - `test_duplicate_class_members` (older, Jan 31): Expected 2 TS2300
  - `test_duplicate_property_then_property` (newer, Feb 3): Expected 1 TS2300
- Verified tsc behavior: Emits exactly 1 TS2300 (on second property) + TS2717

**Resolution**:
- The newer test was correct
- Fixed the older test expectation to match tsc behavior
- Updated test comment to clarify tsc behavior

**Result**: ✅ Conformance improved from 51 to 50 failing tests

## Status: READY FOR NEXT TASK
Test fixed and committed. Ready to pick another simple failing test.
