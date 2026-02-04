# Session tsz-3: CFA Orchestration - Switch Exhaustiveness & Narrowing

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Ensure switch statements correctly narrow types and identify exhausted unions

## Context

Previous session completed all 8 narrowing bug fixes (discriminant, instanceof, in operator). This session builds on that work by focusing on the orchestration layer - how the checker applies narrowing primitives in control flow analysis.

## Problem Statement

While the solver's narrowing primitives are now correct, the checker's orchestration of switch statement narrowing has gaps:
1. **Exhaustiveness**: Not detecting when all union members are covered
2. **Fall-through**: Not handling narrowing across fall-through cases
3. **Default narrowing**: Not narrowing to `never` when union is exhausted
4. **Flow cache**: May not be updating flow-sensitive type cache correctly during switch traversal

## Impact

Correct exhaustiveness checking is critical for:
- Redux-style action patterns
- Algebraic data types
- Type-safe state machines
- Modern TypeScript discriminated union patterns

## Tasks

### Task 1: Switch Narrowing Verification ✅ IN PROGRESS
**File**: `src/checker/control_flow.rs`

**Current Implementation Found**:
- `handle_switch_clause_iterative` (line 560): Main entry point
- `narrow_by_switch_clause` (line 1423): Handles regular cases
- `narrow_by_default_switch_clause` (line 1440): Handles default clause

**How it works**:
1. Regular case: Creates `switch_expr === case_expr` and applies narrowing (true branch)
2. Default case: Loops through all cases and excludes them using `!==` (false branch)
3. Fallthrough: Adds antecedents to worklist (line 612-621)

**What's working**:
- ✅ Regular case narrowing applies discriminant correctly
- ✅ Default clause excludes all previous cases
- ✅ Fallthrough flow control handled

**Gaps identified**:
- ❌ No exhaustiveness detection
- ❌ No narrowing to `never` when union exhausted
- ❌ No type union for fallthrough accumulation

**Status**: ⏸️ Analysis complete, implementation pending

---

### Task 2: Exhaustiveness Detection
**File**: `src/checker/flow_analysis.rs`

Implement logic to detect when a union is fully covered:
- Collect all case discriminant values
- Check if they cover all union members
- When covered, narrow variable to `never` in default/after switch

**Example**:
```typescript
type Action = { type: "add" } | { type: "remove" };
function handle(action: Action) {
  switch (action.type) {
    case "add": /* action is { type: "add" } */
    case "remove": /* action is { type: "remove" } */
    default: /* action should be never here */
  }
  // action should be never here
}
```

**Status**: ⏸️ Not started

---

### Task 3: Fall-through Narrowing
**File**: `src/checker/flow_analysis.rs`

Handle narrowing when cases fall through:
```typescript
switch (x) {
  case 'a':
  case 'b':
    // x should be narrowed to 'a' | 'b'
    break;
}
```

**Status**: ⏸️ Not started

---

### Task 4: Flow Cache Validation
**File**: `src/checker/flow_analysis.rs`

Ensure the checker correctly updates flow-sensitive type cache:
- Each case block should have narrowed type
- Fall-through should accumulate narrowing
- After switch, variable should be correctly narrowed (or never)

**Status**: ⏸️ Not started

---

## Success Criteria

- [ ] Switch statements correctly narrow in each case
- [ ] Exhausted unions narrow to `never` in default/after switch
- [ ] Fall-through cases accumulate narrowing correctly
- [ ] Flow cache is properly updated during switch traversal
- [ ] All conformance tests for switch statements pass

---

## Complexity: MEDIUM-HIGH

**Why Medium-High**:
- `flow_analysis.rs` is complex orchestration code
- Requires understanding FlowNode graph and flow-sensitive typing
- Must coordinate with solver's narrowing primitives
- Edge cases: breaks, returns, throws in switch

**Implementation Principles**:
1. Use the fixed narrowing primitives from solver
2. Respect FlowNode graph structure
3. Follow Two-Question Rule (AGENTS.md)
4. Test with Redux-style patterns

---

## Session History

- 2026-02-04: Previous session completed - all 8 narrowing bugs fixed
- 2026-02-04: Session redefined - focus on switch exhaustiveness and CFA orchestration

## Previous Achievements (Archived)

All narrowing bug fixes completed:
- instanceof narrowing (interface vs class)
- in operator narrowing (unknown, optional, open objects, intersection)
- discriminant narrowing (filtering approach with proper resolution)
