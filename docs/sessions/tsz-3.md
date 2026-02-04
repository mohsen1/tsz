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

### Task 2: Lazy Type Resolution Bug 🐛 PRE-EXISTING BUG DISCOVERED
**File**: `src/solver/narrowing.rs`, `src/solver/visitor.rs`

**Bug Found**: `union_list_id` doesn't resolve Lazy types before checking if type is a union.

**Symptom**:
```
Excluding discriminant value 100 from union with 1 members
```

The Action type should have 2 members (`{ type: "add" }` and `{ type: "remove" }`), but only 1 member is found.

**Root Cause**:
- Type 123 is a `Lazy` type reference to the Action type alias
- `union_list_id` uses `extract_type_data` which calls `visit_lazy`
- Default `visit_lazy` implementation returns `None` instead of resolving the type
- The slice normalization treats the Lazy type as a single member
- Narrowing operates on this single-member "union" and produces no change

**Trace**:
```
type 123 (Action) -> Lazy(DefId) -> should resolve to Union[{add}, {remove}]
But: union_list_id(123) -> None -> slice normalization -> [123] -> 1 member
```

**Status**: 🐛 Pre-existing bug - needs separate fix
**Next**: Modify `union_list_id` or add helper to resolve Lazy types before extracting union members
**Note**: This is NOT specific to switch statements - affects all union narrowing on type aliases

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
