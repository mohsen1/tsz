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
⏸️ 5 failing circular extends tests identified (solver::infer)
🔍 Need to find the actual "3 failing control flow tests" mentioned in session docs

## Next Steps (per Gemini guidance)

1. Search for the 3 failing control flow tests: `cargo nextest run -E 'test(/flow/) or test(/narrowing/)'`
2. Determine if control flow test failures are caused by solver::infer issues
3. If related: Fix solver::infer first (foundational layer)
4. If unrelated: Fix control flow tests independently

### Gemini's Recommendation
If fixing the 5 circular extends tests:
1. **Question 1**: "I need to fix 5 failing tests in solver::infer related to circular extends. My understanding is that the inference engine is failing to find the greatest fixed point/stable bound. What is the correct approach in tsz to handle recursive constraints during inference?"
2. **Question 2** (after implementing): Review the implementation for correctness

## References

- FlowAnalyzer: src/checker/flow_analysis.rs
- ControlFlow: src/checker/control_flow.rs
- apply_flow_narrowing: src/checker/flow_analysis.rs:1320
