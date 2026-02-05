# Session TSZ-2: Array Destructuring Type Narrowing

**Started**: 2026-02-05
**Status**: ⚠️ BLOCKED - Need to find FlowAnalyzer assignment logic

## Goal

Fix type narrowing invalidation for array destructuring assignments.

## Completed Work

✅ Fixed test_truthiness_false_branch_narrows_to_falsy (boolean narrowing bug)
✅ Extended CheckerState::collect_array_destructuring_assignments
   - Added handling for simple identifiers: [x] = [1]
   - Added handling for binary expressions (defaults): [x = 1] = []

## Root Cause Discovery (from Gemini Pro)

**CRITICAL:** I modified the WRONG component!

- ✅ Modified: `CheckerState::collect_array_destructuring_assignments` in `src/checker/flow_analysis.rs`
- ❌ This is for **Definite Assignment** analysis (TS2454 "used before assigned")
- ✅ Need to modify: `FlowAnalyzer` for **Type Narrowing**
- ✅ FlowAnalyzer likely has separate logic that doesn't see my CheckerState changes

## Current Blocker

The failing tests expect narrowing to be cleared when:
- `[x] = [1]` (simple identifier in array destructuring)
- `[x = 1] = []` (array destructuring with default)

But `FlowAnalyzer` doesn't recognize these as assignments that should clear narrowing.

## Error Details

Tests fail with: `TypeId(111) instead of TypeId(130)`
- TypeId(111): The narrowed type (should have been cleared)
- TypeId(130): The full union type (expected after assignment)

## Next Steps (from Gemini Pro Guidance)

1. **Ask Gemini Question 1**: Find where `FlowAnalyzer` handles `FlowNode::Assignment`
   ```bash
   ./scripts/ask-gemini.mjs --include=src/checker --include=src/solver \
   "Where is FlowAnalyzer implemented? Does it handle FlowNode::Assignment?
   Where does it determine which variables to invalidate?"
   ```

2. **Implement Destructuring Support**: Add array destructuring logic to FlowAnalyzer

3. **Verify Tests**: Run the 2 failing tests to confirm fix

## Test Cases

### test_array_destructuring_assignment_clears_narrowing
```typescript
let x: string | number;
if (typeof x === "string") {
  x;           // narrowed to string
  [x] = [1];    // should clear narrowing
  x;           // should be string | number (union)
}
```

### test_array_destructuring_default_initializer_clears_narrowing
```typescript
let x: string | number;
if (typeof x === "string") {
  x;             // narrowed to string
  [x = 1] = [];  // should clear narrowing
  x;             // should be string | number (union)
}
```

## Technical Context

- **src/checker/flow_analysis.rs**: Contains CheckerState (definite assignment)
- **src/checker/control_flow.rs**: Contains FlowAnalyzer (type narrowing)
- These two components have SEPARATE assignment tracking logic
- Modifying one doesn't affect the other
