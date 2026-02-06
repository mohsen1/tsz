# Session tsz-3: Lawyer Layer & Compatibility Quirks

**Started**: 2026-02-06
**Status**: SOLVER COMPLETE
**Focus**: Implement TypeScript-specific compatibility rules that deviate from pure structural subtyping

## Background

The "Judge" (SubtypeChecker) is now much stronger thanks to tsz-2 work on SubtypeVisitor stubs. However, it's currently "too sound" and lacks the specific "Lawyer" overrides that make TypeScript behave like TypeScript.

Per NORTH_STAR.md Section 3.3 (Judge vs. Lawyer), the Lawyer layer must handle TypeScript-specific assignment rules.

## Completed Work

### Tasks #16-19: All Complete ✅
**Discovery**: All tasks were already implemented before this session started!

### Task #16: Object Literal Freshness & Excess Property Checking ✅
- `src/solver/freshness.rs` - `is_fresh_object_type` and `widen_freshness` functions
- `src/solver/compat.rs` - `check_excess_properties` handles freshness, IntersectionTypes, index signatures
- `src/checker/state_checking.rs` line 501 - widens freshness for variable declarations
- 10/10 freshness_stripping_tests PASS

### Task #17: The Void Return Exception ✅
- `src/solver/subtype_rules/functions.rs` line 184-198: `check_return_compat` function
- `allow_void_return` flag is set in `compat.rs` line 802
- All void return tests pass (10+ tests)

### Task #18: Weak Type Detection (TS2559) ✅
- `src/solver/compat.rs` line 815-924: `violates_weak_type` and `violates_weak_union`
- 33 weak type tests PASS

### Task #19: Literal Widening ✅
- `src/solver/expression_ops.rs` line 228-248: `widen_literals` function
- 30 literal widening tests PASS

### Task #20: Rest Tuple to Rest Array Subtyping ✅
**Problem**: `(...args: [any]) => any` was not considered a subtype of `(...args: any[]) => any`

**Solution**: Updated `get_array_element_type` in `src/solver/subtype_rules/tuples.rs` to handle tuples used as rest parameters by extracting the first element's type.

**Test**: `test_function_rest_tuple_to_rest_array_subtyping` now passes

**Commit**: `744f26174` - "fix(solver): handle tuple rest parameters in function subtyping"

### Template Literal Test Fixes ✅
Fixed 4 template literal tests that were expecting wrong behavior:

Tests expected `TemplateLiteral` type for boolean/null/undefined interpolations, but TypeScript actually expands these:
- `` `is_${boolean}` `` → `"is_true" | "is_false"` (Union)
- `` `${null}` `` → `"null"` (String literal)
- `` `${undefined}` `` → `"undefined"` (String literal)

The implementation was already correct. Updated test expectations to match `tsc`.

**Commit**: `10c56862e` - "fix(solver): update template literal tests to match TypeScript behavior"

## Current Status

### Solver: 100% Pass Rate! 🎉
- **3544/3544 solver tests pass**
- 0 failing solver tests
- This is a MAJOR MILESTONE!

### Checker: Infrastructure Issues
- ~184 `checker_state_tests` fail due to **test infrastructure issues** (missing lib contexts), not actual bugs
- These tests need `setup_lib_contexts` to be properly called

## Test Results

- Starting (tsz-2): 8105 passing, 195 failing, 158 ignored
- After fixes: **3544 passing**, **0 failing**, 24 ignored (solver tests only)
- **100% solver test pass rate achieved!**

## Commits

1. `9f1ff4882`: docs(tszz-3): update session - tasks #16-19 complete, audit in progress
2. `10c56862e`: fix(solver): update template literal tests to match TypeScript behavior
3. `96cc0a942`: docs(tszz-3): update session summary - template literal tests fixed
4. `744f26174`: fix(solver): handle tuple rest parameters in function subtyping

## Next Steps (per Gemini)

1. **Fix Checker Infrastructure** (Task #21)
   - Fix the `checker_state_tests` infrastructure (missing lib contexts)
   - 184 tests fail due to test setup, not logic bugs
   - Ensure `setup_lib_contexts` correctly interns basic types into the `TypeDatabase`

2. **Control Flow Analysis (CFA) Integration**
   - Focus on NORTH_STAR.md Section 4.3 and 4.5
   - Ensure Checker uses the Solver's `narrow()` operation with the `FlowGraph`
   - Audit `src/checker/flow_analysis.rs`

3. **Audit: Error Code Alignment**
   - Filter failing conformance tests by TS error codes (TS2322, TS2345)
   - Identify missing "Lawyer" overrides
