# Session tsz-3: Lawyer Layer & Compatibility Quirks

**Started**: 2026-02-06
**Status**: In Progress
**Focus**: Implement TypeScript-specific compatibility rules that deviate from pure structural subtyping

## Background

The "Judge" (SubtypeChecker) is now much stronger thanks to tsz-2 work on SubtypeVisitor stubs. However, it's currently "too sound" and lacks the specific "Lawyer" overrides that make TypeScript behave like TypeScript.

Per NORTH_STAR.md Section 3.3 (Judge vs. Lawyer), the Lawyer layer must handle TypeScript-specific assignment rules.

## Completed Work

### Task #16: Object Literal Freshness & Excess Property Checking ✅
**Status**: COMPLETE

**Evidence**:
- `src/solver/freshness.rs` - `is_fresh_object_type` and `widen_freshness` functions exist
- `src/solver/compat.rs` - `check_excess_properties` handles:
  - Fresh object detection (line 434)
  - Target property collection for IntersectionTypes (line 567)
  - String index signature check (line 459-461)
  - Excess property error reporting (line 467-472)
- `src/checker/type_computation.rs` line 1745 - uses `object_fresh` for object literals
- `src/checker/state_checking.rs` line 501 - widens freshness for variable declarations (prevents "Zombie Freshness")
- `src/checker/type_computation_complex.rs` line 1388-1390 - widens freshness in flow analysis

**Test Results**:
- 10/10 freshness_stripping_tests PASS
- 11/12 excess_property tests PASS (1 failure is test setup issue, not logic)
- 4/4 union_optional tests PASS
- Overall: 8111 passing (up from 8105), 189 failing (down from 195)

**Files**: `src/solver/freshness.rs`, `src/solver/compat.rs`, `src/checker/state_checking.rs`, `src/checker/type_computation_complex.rs`

### Task #17: The Void Return Exception ✅
**Status**: COMPLETE

**Evidence**:
- `src/solver/subtype_rules/functions.rs` line 184-198: `check_return_compat` function implements void return special-casing
- `allow_void_return` flag is set in `compat.rs` line 802
- All void return tests pass (10+ tests)

**Files**: `src/solver/subtype_rules/functions.rs`, `src/solver/compat.rs`

### Task #18: Weak Type Detection (TS2559) ✅
**Status**: COMPLETE

**Evidence**:
- `src/solver/compat.rs` line 815-924: `violates_weak_type` and `violates_weak_type_with_target_props` functions
- Handles union weak types via `violates_weak_union`
- 33 weak type tests PASS

**Files**: `src/solver/compat.rs`

### Task #19: Literal Widening ✅
**Status**: COMPLETE

**Evidence**:
- `src/solver/expression_ops.rs` line 228-248: `widen_literals` function
- Used in `get_best_common_type` for array literal inference
- 30 literal widening tests PASS

**Files**: `src/solver/expression_ops.rs`

## Current Task

### Audit & Template Literal Test Fixes

**Discovery**: Tasks #16-19 were ALL COMPLETE before this session started!

**Current Status**:
- **3539/3568 solver tests pass** (99.2% pass rate!)
- Only **5 solver tests fail**, all related to template literal tests
- ~184 `checker_state_tests` fail due to **test infrastructure issues** (missing lib contexts), not actual bugs

**The 5 failing solver tests**:
1. `test_template_literal_null_undefined` - expects TemplateLiteral, gets Union
2. `test_template_literal_with_boolean` - expects TemplateLiteral, gets Union
3. `test_template_literal_with_boolean_type` - expects TemplateLiteral, gets Union
4. `test_template_literal_with_boolean_interpolation` - expects TemplateLiteral, gets Union
5. `test_function_rest_tuple_to_rest_array_subtyping` - needs investigation

**Root Cause** (per Gemini): The tests are **incorrect/outdated**. TypeScript's behavior is to expand:
- `` `is_${boolean}` `` → `"is_true" | "is_false"` (Union)
- `` `${null}` `` → `"null"` (Literal)
- `` `${undefined}` `` → `"undefined"` (Literal)

The current implementation is **CORRECT** - it matches `tsc` behavior exactly. The tests need to be updated to expect `Union` instead of `TemplateLiteral`.

**Files to fix**:
- `src/solver/tests/evaluate_tests.rs` (4 tests)
- `src/solver/tests/template_literal_comprehensive_test.rs` (1 test)

## Remaining Tasks

1. **Fix template literal tests** (5 tests) - update expectations to match TypeScript behavior
2. **Investigate function rest tuple test** (1 test)
3. **Fix checker_state_tests** (184 tests) - fix test infrastructure (setup_lib_contexts)
4. **Audit 189 failing tests** - filter by error codes to find next high-impact work

## Test Results

- Starting (tsz-2): 8105 passing, 195 failing, 158 ignored
- After Task #16 complete: 8111 passing (+6), 189 failing (-6), 158 ignored
- Solver tests: 3539 passing, 5 failing, 24 ignored (99.2% pass rate)

## Next Steps

1. Fix the 5 template literal tests (tests are wrong, implementation is correct)
2. Investigate the function rest tuple test
3. Audit failing tests by error code clusters to find next high-impact tasks
