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

### Investigation Findings

**Step 1: Locate TypeGuard Extraction** ✅ COMPLETE

Found in `src/checker/control_flow_narrowing.rs:1779`:
- Function `extract_type_guard` extracts type guards from binary expressions
- Expects condition to be a binary expression like `x.done === true`
- Line 1784: `let bin = self.arena.get_binary_expr(cond_node)?;`

**The Problem**: Our test has `(o = fn()).done` which is NOT a simple binary expression:
- It's an assignment expression: `o = fn()`
- Wrapped in property access: `.done`
- The function expects a direct binary expression, not a property access on an assignment

**Step 2: Flow Analysis** ✅ INVESTIGATED

In `src/checker/flow_analysis.rs:215-240`:
- Line 218-222: Calls `collect_assignments_in_expression` on if condition
- This tracks assignments in the condition for definite assignment analysis
- But doesn't extract type guards for narrowing

**Missing Link**: The type guard extraction happens elsewhere, and it needs to handle:
1. Unwrapping assignment expressions to get the actual value
2. Handling property access expressions as discriminant checks
3. Connecting the flow analysis with the narrowing system

### Root Cause

The type guard extraction code (`extract_type_guard`) doesn't handle complex expressions like:
- `(x = val).prop` - assignment with property access
- It expects simple binary expressions like `x.prop === value`

### Next Investigation Step

Find where `extract_type_guard` is called and understand the flow from:
IF statement → condition expression → type guard extraction → narrowing application

### Key Files
- `src/checker/control_flow*.rs` - Type guard extraction
- `src/solver/narrowing.rs` - Discriminant narrowing implementation
- `src/tests/checker_state_tests.rs:22821` - Test location
