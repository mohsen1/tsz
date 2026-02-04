# Session tsz-3 - Control Flow Analysis Fixes

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Type Evaluation & Flow Analysis

## Completed Work

✅ **Global Symbol Resolution (TS2304 Poisoning Fix)**
- Fixed lib_contexts fallback in symbol resolver
- Array globals now correctly report TS2339 for non-existent properties
- Commit: `031b39fde`

✅ **instanceof Narrowing Implementation**
- Implemented `narrow_by_instanceof` method in `src/solver/narrowing.rs`
- Uses `classify_for_instance_type` to extract instance type from constructor
- Handles Callable, Function, Intersection, Union, Readonly, TypeParameter
- Supports both positive and negative narrowing
- Test `test_instanceof_narrows_to_object_union_members` passes
- Commit: `bcfb9d6a9`

✅ **in Operator Narrowing Implementation**
- Implemented `narrow_by_property_presence` method in `src/solver/narrowing.rs`
- Added `type_has_property` helper to check if a type has a property
- Handles object shapes, index signatures, and union filtering
- Supports both positive (`"prop" in x`) and negative (`!("prop" in x)`) narrowing
- Test `test_in_operator_narrows_required_property` passes
- Commit: `9d6da2af7`

✅ **Truthiness Narrowing Verification**
- Verified that `narrow_by_truthiness` correctly matches TypeScript behavior
- TypeScript only removes `null` and `undefined` in truthiness checks
- TypeScript does NOT narrow literal types like `false`, `0`, `""` based on truthiness
- Updated documentation to clarify expected behavior
- Behavior now matches tsc exactly

✅ **Tail-Recursive Conditional Type Evaluation Fix**
- Fixed depth limit bug in tail-recursion elimination
- Check if result is conditional BEFORE calling evaluate (avoids depth increment)
- Test `test_tail_recursive_conditional` now passes
- Commit: `6b20e0180`

## Current Task: Investigate Assignment Expression Narrowing

### Problem Statement

Test `test_assignment_expression_condition_narrows_discriminant` fails with TS2322 error.

**Test Code**:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
declare function fn(): D;
let o: D;
if ((o = fn()).done) {
    const y: 1 = o.value; // Should work - o should be narrowed to { done: true, value: 1 }
}
```

**Expected**: No errors (narrowing works)
**Actual**: TS2322 error - `o.value` is not narrowed to `1`

**Root Cause (Hypothesis)**: The flow analysis doesn't properly track narrowing when:
1. Assignment happens in the condition expression (`o = fn()`)
2. Property access (`o.done`) narrows the type
3. The narrowed type should persist into the if block

### Investigation Plan

1. Check how flow analysis tracks assignments in condition expressions
2. Verify if discriminant narrowing is being applied to the result of the assignment
3. Check if the narrowed type is being propagated to the if block

### Key Files
- `src/checker/flow_analysis.rs` - Flow analysis implementation
- `src/checker/control_flow*.rs` - Control flow tracking
- `src/solver/narrowing.rs` - Discriminant narrowing logic
- `src/tests/checker_state_tests.rs` - Test at line 22821
