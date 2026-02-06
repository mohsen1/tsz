# Session TSZ-12: Cache Invalidation & Test Infrastructure Cleanup

**Started**: 2026-02-06
**Status**: 🔄 NOT STARTED
**Predecessor**: TSZ-11 (Readonly Type Support - Complete)

## Task

Fix cache invalidation issues and test infrastructure problems to resolve ~20 failing tests.

## Problem Statement

### Part A: Cache Invalidation (~14 tests)

The `CheckerContext` and `Solver` maintain memoization tables (like `node_types` and `symbol_types`) that may:
- Persist across test cases when they shouldn't
- Fail to account for contextual changes (generic instantiations, flow-sensitive states)
- Return stale data causing incorrect type checking results

**Tests affected**:
- `compile_with_cache_emits_only_dirty_files`
- `compile_with_cache_invalidates_dependents`
- `invalidate_paths_with_dependents_symbols_*`

### Part B: Readonly Infrastructure (~6 tests)

Readonly test failures are due to lib infrastructure issues, not subtyping logic (which was fixed in tsz-11):
- Tests manually create `CheckerState` without loading lib files
- Missing `Array`, `ReadonlyArray`, or `readonly` keyword support
- Test setup incomplete

**Tests affected**:
- `test_readonly_array_element_assignment_2540`
- `test_readonly_element_access_assignment_2540`
- `test_readonly_index_signature_element_access_assignment_2540`

## Expected Impact

- **Direct**: Fix ~20 tests (14 cache + 6 infrastructure)
- **Indirect**: Provide clean slate for complex features (overload resolution, flow narrowing)
- **Percentage**: Resolves ~30% of current 68 failures

## Files to Investigate

### Part A: Cache Invalidation
1. **src/checker/context.rs** - Review `CheckerContext` cache fields
2. **src/checker/state.rs** - Ensure caches are properly reset
3. **src/solver/mod.rs** - Check solver-level caching

### Part B: Readonly Infrastructure
1. **src/solver/intern.rs** - Ensure `ReadonlyArray` intrinsics are mapped
2. **src/tests/checker_state_tests.rs** - Fix test setup to use lib fixtures

## Implementation Plan

### Phase 1: Investigate Cache Invalidation

Ask Gemini:
1. What is the root cause of cache invalidation failures?
2. Which caches should be global vs local?
3. How to properly reset caches between test runs?
4. Are there specific functions that need to invalidate cache entries?

### Phase 2: Fix Cache Logic

Based on Gemini's guidance:
- Identify stale cache sources
- Implement proper cache clearing
- Add cache invalidation on context changes

### Phase 3: Fix Readonly Test Infrastructure

Update failing tests to use proper lib setup:
- Use test fixtures that load lib files
- Ensure `ReadonlyArray` types are available
- Verify tests actually test what they claim to test

### Phase 4: Test and Commit

Run conformance tests and commit fixes.

## Test Status

**Start**: 8232 passing, 68 failing

## Notes

**Gemini's Recommendation**: "Fixing cache invalidation addresses ~14 tests. Combined with the Readonly infrastructure fixes (~6 tests), this single session could resolve 20 out of 68 remaining failures (~30% of the current failure set)."

**Rationale**:
- Highest impact of remaining features
- More straightforward than overload resolution or flow narrowing
- Provides clean slate for complex type system features
- Unblocks other features by removing non-deterministic failures

**Deferred Features**:
- Flow Narrowing: Requires robust CFG (deferred from tsz-10)
- Overload Resolution: High impact but high complexity (recommended for tsz-13)
