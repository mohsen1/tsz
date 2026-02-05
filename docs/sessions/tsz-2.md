# Session TSZ-2: Array Destructuring Type Narrowing

**Started**: 2026-02-05
**Status**: 🔬 DEEP DEBUGGING - Following Gemini Pro guidance

## Goal

Fix type narrowing invalidation for array destructuring assignments.

## Progress

✅ **Completed**: test_truthiness_false_branch_narrows_to_falsy (boolean narrowing bug)
⚠️ **Blocked**: 2 array destructuring tests (code looks correct but tests fail)

## Gemini Pro Assessment

"This is a classic 'logic looks correct but behavior is wrong' scenario. **Do not abandon this.** Correct control flow analysis is non-negotiable for the 'Match tsc' goal."

**Recommendation**: Continue debugging - likely very close to solution.

## Debugging Strategy (from Gemini Pro)

### Step 1: Verify Flow Node Creation
Check if ASSIGNMENT flow node is created for destructuring:
```bash
TSZ_LOG="wasm::checker::flow_graph_builder=debug" TSZ_LOG_FORMAT=tree \
  cargo test test_array_destructuring_assignment_clears_narrowing
```

### Step 2: Verify Target Matching
Add tracing to `assignment_targets_reference_node`:
```rust
tracing::debug!(is_op=?is_op, targets=?targets,
  "Checking binary assignment target");
```

### Step 3: Verify Internal Recursion
Check if `assignment_targets_reference_internal` correctly matches `[x]` against `x`

### Step 4: Analyze Results
- If `targets_reference` is `false`: Bug in matching logic
- If `targets_reference` is `true`: Bug in `check_flow` or `get_assigned_type`

## Test Cases

### test_array_destructuring_assignment_clears_narrowing
```typescript
let x: string | number;
if (typeof x === "string") {
  x;           // narrowed to string
  [x] = [1];    // should clear narrowing
  x;           // expected: string | number, actual: string
}
```

### test_array_destructuring_default_initializer_clears_narrowing
```typescript
let x: string | number;
if (typeof x === "string") {
  x;             // narrowed to string
  [x = 1] = [];  // should clear narrowing
  x;             // expected: string | number, actual: string
}
```

## Code Locations Found

- `src/checker/control_flow.rs:536-575`: Assignment handling in `check_flow`
- `src/checker/control_flow.rs:1591-1653`: `assignment_targets_reference_node`
- `src/checker/control_flow_narrowing.rs:138-232`: `assignment_targets_reference_internal`
- `src/checker/control_flow.rs:1261-1400`: `match_destructuring_rhs`
- `src/checker/flow_graph_builder.rs:1341-1427`: `handle_expression_for_assignments`

## Next Actions

1. Add tracing to identify where the logic breaks
2. Run test with TSZ_LOG to capture execution
3. Ask Gemini Pro specific question about recursion edge cases
4. Fix the identified issue
