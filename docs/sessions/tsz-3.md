# Session tsz-3 - Control Flow Analysis Fixes

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Assignment Expression Discriminant Narrowing

## Context

Previous session (tsz-3-control-flow-analysis) completed:
- instanceof narrowing
- in operator narrowing
- Truthiness narrowing verification
- Tail-recursive conditional type evaluation fix

Gemini recommended working on `test_assignment_expression_condition_narrows_discriminant` next.

## Current Task: Assignment Expression Condition Narrows Discriminant

### Problem Statement

Test `test_assignment_expression_condition_narrows_discriminant` fails with TS2322 error.

**Test Case**:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
declare function fn(): D;
let o: D;
if ((o = fn()).done) {
    const y: 1 = o.value; // Should work - o should be narrowed to { done: true, value: 1 }
}
```

**Expected**: No errors (narrowing works)
**Actual**: TS2322 error - `o.value` is type `1 | 2` instead of `1`

### Investigation Plan (from Gemini)

1. **Locate Guard Extraction Logic**: Find where `TypeGuard`s are created in the checker
   - Likely in `src/checker/control_flow*.rs` or `src/checker/type_checking.rs`
   - Checker needs to recognize patterns like `if ((x = val).kind === "A")`

2. **Verify Solver Support**: Check `src/solver/narrowing.rs`
   - `narrow_by_discriminant` function (lines 268-297) - handles the "WHAT" part
   - `find_discriminants` (lines 174-263) - finds discriminant properties

3. **Debug Path**: Use tracing to check if `narrow_by_discriminant` is called
   - If not called → issue is upstream in checker's flow analysis
   - If called but returns wrong type → issue in narrowing logic

### Key Files
- `src/checker/control_flow*.rs` - Type guard extraction
- `src/solver/narrowing.rs` - Discriminant narrowing implementation
- `src/tests/checker_state_tests.rs:22821` - Test location
