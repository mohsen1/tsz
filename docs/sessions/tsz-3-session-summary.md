# Session tsz-3: Lawyer Layer & Compatibility Quirks

**Started**: 2026-02-06
**Status**: In Progress
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

## Current Task

### Template Literal Test Fixes ✅

**Fixed 4 template literal tests** - tests were expecting wrong behavior:

Tests expected `TemplateLiteral` type for boolean/null/undefined interpolations, but TypeScript actually expands these:
- `` `is_${boolean}` `` → `"is_true" | "is_false"` (Union)
- `` `${null}` `` → `"null"` (String literal)
- `` `${undefined}` `` → `"undefined"` (String literal)

The implementation was already correct. Updated test expectations to match `tsc`.

**Fixed tests**:
- `test_template_literal_null_undefined`
- `test_template_literal_with_boolean_type`
- `test_template_literal_with_boolean_interpolation`
- `test_template_literal_with_boolean` (template_literal_comprehensive_test.rs)

**Commit**: `10c56862e` - "fix(solver): update template literal tests to match TypeScript behavior"

## Current Status

- **3543/3568 solver tests pass** (99.3% pass rate!)
- **Only 1 solver test fails**: `test_function_rest_tuple_to_rest_array_subtyping`
- ~184 `checker_state_tests` fail due to **test infrastructure issues** (missing lib contexts), not actual bugs

**Remaining solver test**:
- `test_function_rest_tuple_to_rest_array_subtyping` - Tests that `(...args: [any]) => any` is a subtype of `(...args: any[]) => any`

## Test Results

- Starting (tsz-2): 8105 passing, 195 failing, 158 ignored
- After template literal fixes: **3543 passing**, **1 failing**, 24 ignored (solver tests only)

## Next Steps

1. **Investigate function rest tuple test** - 1 remaining solver test failure
2. **Fix checker_state_tests** - fix test infrastructure (setup_lib_contexts) - 184 tests
3. **Audit failing tests** - filter by error codes to find next high-impact work
