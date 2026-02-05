# Session TSZ-2: Control Flow Test Fixes

**Started**: 2026-02-05
**Status**: 🐞 DEBUGGING

## Goal

Fix the 3 failing control flow tests to ensure `instanceof` narrowing works correctly.

## Context

- CI is now green (formatting ✅, clippy ✅)
- 3 failing tests remain in `control_flow` module
- Flow narrowing infrastructure IS already wired up (discovered in TSZ-11)

## Failing Tests Discovered

### 5 Failing Circular Extends Tests (solver::infer):
1. `test_circular_extends_chain_with_endpoint_bound` - assertion failed: expected TypeParameter but got something else
2. `test_circular_extends_conflicting_lower_bounds` - assertion failed: TypeId mismatch (130 vs 9)
3. `test_circular_extends_three_way_with_one_lower_bound` - assertion failed: TypeId mismatch (8 vs 3)
4. `test_circular_extends_with_concrete_upper_and_lower` - assertion failed: TypeId mismatch (10 vs 114)
5. `test_circular_extends_with_literal_types` - assertion failed: TypeId mismatch (10 vs 113)

### 1 Timeout:
- `test_template_literal_expansion_limit_widens_to_string` - TIMEOUT after 5s

### 1 Stack Overflow:
- `test_interface_extends_class_no_recursion_crash` - ironic name, actual stack overflow

### Status of "3 Failing Control Flow Tests":
**NOT FOUND YET** - Need to search for these specifically. They may be:
- In `src/checker/flow_analysis.rs` tests
- In `src/solver/narrowing.rs` tests
- Or the documentation was outdated

## Current Status

✅ Fixed compilation errors in operations_tests.rs (replaced deprecated `with_resolver` API)
✅ FIXED: test_truthiness_false_branch_narrows_to_falsy (boolean narrowing bug)
   - Bug: narrow_to_falsy grouped boolean with string/number/bigint
   - Fix: Separate handling - boolean → BOOLEAN_FALSE, others stay as-is
⏳ IN PROGRESS: 2 remaining array destructuring tests
   - test_array_destructuring_default_initializer_clears_narrowing
   - test_array_destructuring_assignment_clears_narrowing
   - Added handling for identifiers and binary expressions in collect_array_destructuring_assignments
   - Tests still failing - issue is likely in how assignments are applied, not collected
⏸️ 5 failing circular extends tests identified (solver::infer) - NOT STARTED

## Latest Changes (2026-02-05)

### Extended collect_array_destructuring_assignments
Added support for:
1. Simple identifiers: `[x] = [1]` - should clear narrowing on x
2. Assignment expressions with defaults: `[x = 1] = []` - should clear narrowing on x

### Tests Still Failing
Both array destructuring tests still fail with TypeId mismatch. The assignments are being collected correctly, but the narrowing is not being cleared. This suggests the issue is in how the `assigned` set is applied to clear narrowing, possibly in `get_flow_type` or the flow node resolution.

## Session Progress Summary

### Completed Work:
1. Fixed deprecated `with_resolver` API usage in operations_tests.rs
2. Identified and fixed boolean narrowing bug in narrow_to_falsy
3. Updated test expectations to match TypeScript behavior

### Remaining Work:
1. Fix array destructuring assignment clearing (2 tests)
2. Fix 5 circular extends tests in solver::infer
3. Find and fix the "3 failing control flow tests" if different from above

## Next Steps (per Gemini guidance)

**Priority: Finish the remaining 2 array destructuring tests**

1. Investigate `collect_array_destructuring_assignments` in flow_analysis.rs
2. Check if it handles AssignmentPattern (default values) correctly
3. Use tracing: `TSZ_LOG="wasm::checker::flow_analysis=trace" cargo test <test_name>`
4. Ask Gemini Question 1 before implementing fix
5. DO NOT work on solver::infer circular extends tests (defer to next session)

**Gemini's Analysis:**
- The issue is likely that FlowAnalyzer doesn't identify variables as "definitely assigned"
  when they have default values in destructuring patterns
- Need to check if `collect_array_destructuring_assignments` recurses into AssignmentPatterns
- May need to modify how the binder marks these as assigned

### Gemini's Recommendation
If fixing the 5 circular extends tests:
1. **Question 1**: "I need to fix 5 failing tests in solver::infer related to circular extends. My understanding is that the inference engine is failing to find the greatest fixed point/stable bound. What is the correct approach in tsz to handle recursive constraints during inference?"
2. **Question 2** (after implementing): Review the implementation for correctness

## References

- FlowAnalyzer: src/checker/flow_analysis.rs
- ControlFlow: src/checker/control_flow.rs
- apply_flow_narrowing: src/checker/flow_analysis.rs:1320
