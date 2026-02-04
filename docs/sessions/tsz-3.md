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

### Task 2: Exhaustiveness Detection 🐛 BUG FOUND
**File**: `src/checker/control_flow.rs`

**Gemini Guidance (Question 1 Response)**:
- ✅ Current `narrow_by_default_switch_clause` is **semantically correct**
- ✅ Uses subtraction: `T \ C1 \ C2...` which naturally returns `never` when exhausted
- ⚠️ Can be optimized: batch subtraction `T \ {C1, C2...}` instead of iterative
- ❌ Don't manually "detect" exhaustion - let Solver's subtraction do it
- ❌ Don't modify FlowNode structure - not needed

**Test Result - BUG FOUND**:
Test case: `type Action = { type: "add" } | { type: "remove" }` with switch covering both cases.

**Expected**: Default clause should narrow to `never`
**Actual**: Default clause still sees `Action` (not narrowed)
**Error**: `Type 'Action' is not assignable to type 'never'`

**Debug Output**:
```
DEBUG apply_flow_narrowing: result=123
// result is still original Action type (123), not never
```

**Root Cause Analysis Needed**:
The `narrow_by_default_switch_clause` function (line 1440-1476) iterates through cases and subtracts them, but the result is not narrowing to `never` as expected.

**Status**: 🐛 Bug found - exhaustiveness not working despite correct-looking code
**Next**: Debug why subtraction isn't producing `never`

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
- 2026-02-04: Fixed cache bug - SWITCH_CLAUSE nodes now skip cache to enable proper traversal
- 2026-02-04: **BINDER BUG FOUND**: `get_switch_for_clause` returns None for default clauses

## Root Cause Identified 🐛

**File**: `src/checker/control_flow.rs`
**Function**: `handle_switch_clause_iterative` (line 582)

**Bug**: `binder.get_switch_for_clause(clause_idx)` returns `None`
- flow.node=34 (default clause)
- Expected: Should return the switch statement node index
- Actual: Returns None, causing early return before narrowing logic

**Impact**: Switch exhaustiveness cannot work because the binder doesn't associate default clause flow nodes with their switch statements.

**Status**: 🐛 Root cause found - requires binder investigation

## Next Steps

Ask Gemini to investigate binder's get_switch_for_clause function to understand why it's not finding the switch for default clauses.

## Previous Achievements (Archived)

All narrowing bug fixes completed:
- instanceof narrowing (interface vs class)
- in operator narrowing (unknown, optional, open objects, intersection)
- discriminant narrowing (filtering approach with proper resolution)
