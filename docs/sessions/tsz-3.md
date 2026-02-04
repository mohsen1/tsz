# Session tsz-3: CFA Orchestration - Switch Exhaustiveness & Narrowing

**Started**: 2026-02-04
**Status**: ✅ SWITCH EXHAUSTIVENESS COMPLETE
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

### Task 1: Switch Narrowing Verification ✅ COMPLETE (Fixed switch clause mapping)
**File**: `src/checker/control_flow.rs`, `src/binder/state_binding.rs`, `src/parallel.rs`

**Fixed Issue**: The binder's `switch_clause_to_switch` map was not preserved through the binding/checking pipeline.

**Root Cause**:
- `bind_switch_statement` populated `switch_clause_to_switch` during binding
- This map was NOT included in `BindResult` or `BoundFile`
- During checking, a new `BinderState` was created with an empty map
- `get_switch_for_clause` returned `None`, causing early return

**Solution** (Commit: bdac7f8df):
1. Added `switch_clause_to_switch: FxHashMap<u32, NodeIndex>` to `BindResult` and `BoundFile`
2. Populate the map when creating `BindResult` from binder using `std::mem::take`
3. Include the map when creating `BoundFile` from `BindResult`
4. Pass the map to `from_bound_state_with_scopes_and_augmentations`
5. Updated all 3 call sites in `src/parallel.rs` and `src/cli/driver.rs`

**Result**: ✅ `handle_switch_clause_iterative` is now being called correctly!
The narrowing functions are invoked and the discriminant narrowing logic runs.

**Status**: ✅ Switch clause mapping fixed - commit pushed to main

---

### Task 2: Lazy Type Resolution Bug ✅ FIXED (Commit: fd12bb38e)
**File**: `src/solver/narrowing.rs`, `src/checker/control_flow.rs`, `src/solver/flow_analysis.rs`

**Bug Fixed**: Lazy types (type aliases) were not being resolved before union narrowing.

**Root Cause**:
- `NarrowingContext` used `TypeDatabase` instead of `QueryDatabase`
- Couldn't call `evaluate_type()` to resolve Lazy types to their underlying union types
- Type aliases like `type Action = { type: "add" } | { type: "remove" }` were treated as single members

**Solution**:
1. Changed `NarrowingContext` to use `QueryDatabase` instead of `TypeDatabase`
2. Added `resolve_type()` helper that evaluates Lazy types before narrowing
3. Updated `narrow_by_discriminant` and `narrow_by_excluding_discriminant` to call
   `resolve_type()` before checking for union members
4. Updated `FlowAnalyzer`, `FlowTypeEvaluator` to use `QueryDatabase`

**Test Result**: ✅ Switch exhaustiveness now works!
```typescript
type Action = { type: "add" } | { type: "remove" };

function handle(action: Action) {
  switch (action.type) {
    case "add": break;
    case "remove": break;
    default:
      const impossible: never = action; // Now works correctly!
  }
}
```

**Impact**: This fix affects ALL union narrowing operations, not just switch statements.
- Discriminant narrowing on type aliases now works
- Control flow analysis with type aliases now works correctly
- If/guard narrowing with type aliases now works correctly

**Status**: ✅ Complete - commit pushed to main

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

- [x] Switch statements correctly narrow in each case
- [x] Exhausted unions narrow to `never` in default/after switch
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
