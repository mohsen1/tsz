# Session tsz-3: CFA Orchestration - Fall-through & Loop Narrowing

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Ensure control flow analysis handles fall-through cases and loops correctly

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

### ✅ COMPLETED: Switch Exhaustiveness (2026-02-04)

#### Task 1: Switch Clause Mapping ✅ COMPLETE (Commit: bdac7f8df)
**Fixed Issue**: The binder's `switch_clause_to_switch` map was lost during binding/checking pipeline.

#### Task 2: Lazy Type Resolution ✅ COMPLETE (Commit: fd12bb38e)
**Bug Fixed**: Lazy types (type aliases) were not being resolved before union narrowing.

**Test Result**:
```typescript
type Action = { type: "add" } | { type: "remove" };
function handle(action: Action) {
  switch (action.type) {
    case "add": break;
    case "remove": break;
    default:
      const impossible: never = action; // ✅ Works!
  }
}
```

---

### 🔄 CURRENT TASK: Fall-through & Loop Narrowing

#### Task 3: Fall-through Narrowing (HIGH PRIORITY)
**Goal**: Ensure fall-through cases correctly union narrowed types
**Test Case**:
```typescript
switch (x) {
  case 'a':
  case 'b':
    // x should be narrowed to 'a' | 'b'
    break;
}
```
**File**: `src/checker/control_flow.rs`
**Status**: ✅ PARTIALLY COMPLETE

**Findings**:
1. ✅ **Fall-through union WORKS for literal types** (e.g., `"a" | "b" | "c"`)
   - Test: `test_fallthrough_simple.ts` passes
   - The code in `check_flow` (lines 533-563) correctly unions fallthrough antecedent types

2. ❌ **Fall-through FAILS for type aliases (Lazy types)**
   - Test: `test_fallthrough.ts` fails on line 9 with "never" error
   - Root cause: `evaluate_type()` returns ERROR for Lazy types in narrowing context
   - This is a SEPARATE bug from fall-through union logic

**Technical Details**:
- Fall-through union is implemented in `src/checker/control_flow.rs:533-563`
- For switch clauses with multiple antecedents, the code unions `result_type` with types from `antecedent[1..]`
- The union logic works correctly - the bug is in discriminant narrowing returning `never`

**Related Issue**: Lazy type resolution in discriminant narrowing
- When narrowing `type Action = { type: "add" } | { type: "remove" }` by `action.type === "add"`
- The `resolve_type()` call in `narrow_by_discriminant` evaluates the Lazy type to ERROR
- This causes narrowing to return `never` instead of the specific union member
- This affects ALL discriminant narrowing on type aliases, not just fall-through

**Root Cause Analysis** (from Gemini):
- `QueryDatabase::evaluate_type()` uses `NoopResolver` which can't resolve Lazy types
- The fix is to use `TypeEvaluator::with_resolver()` instead of `TypeEvaluator::new()`
- Need to update `BinderTypeDatabase` to implement `TypeResolver` and use the correct evaluator
- Reference: `src/solver/evaluate.rs:135` and `src/solver/db.rs:448`

**Note**: Exhaustiveness works (commit fd12bb38e) because it doesn't require discriminant narrowing to resolve individual members - it just checks if all cases are covered. Fall-through narrowing requires actual narrowing, which exposes the Lazy type resolution bug.

#### Task 4: Loop Narrowing (HIGH PRIORITY)
**Goal**: Implement narrowing propagation for while/for loops
**Test Case**:
```typescript
while (x.type === 'a') {
  // x should be narrowed to 'a'
}
```
**File**: `src/solver/flow_analysis.rs`
**Status**: ⏸️ Not started

#### Task 5: CFA Completeness Validation
**Goal**: Validate that flow cache is correctly updated during complex CFA traversal
**File**: `src/checker/flow_analysis.rs`
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

- [x] Switch statements correctly narrow in each case (for non-Lazy types)
- [x] Exhausted unions narrow to `never` in default/after switch
- [x] Fall-through cases accumulate narrowing correctly (for literal types)
- [ ] Fall-through narrowing works for type aliases (Lazy types) - BLOCKED by evaluate_type bug
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

## Next Steps

### Option A: Fix Lazy Type Resolution (HIGH IMPACT)
**Problem**: `QueryDatabase::evaluate_type()` can't resolve Lazy types because it uses `NoopResolver`

**Files to modify**:
1. `src/solver/db.rs`: Update `BinderTypeDatabase` to implement `TypeResolver`
2. `src/solver/db.rs`: Change `BinderTypeDatabase::evaluate_type()` to use `TypeEvaluator::with_resolver()`
3. `src/solver/evaluate.rs`: Consider renaming `evaluate_type()` to `evaluate_pure_type()` to prevent misuse

**Impact**: Fixes ALL discriminant narrowing on type aliases, not just fall-through

**Test**: `test_fallthrough.ts` should pass after fix

### Option B: Focus on Loop Narrowing (MEDIUM IMPACT)
**Task 4**: Implement narrowing propagation for while/for loops

**File**: `src/solver/flow_analysis.rs`

**Test case**:
```typescript
while (x.type === 'a') {
  // x should be narrowed to 'a'
}
```

**Status**: ⏸️ Not started

### Option C: Redefine Session
If Lazy type resolution is too complex for this session, redefine tsz-3 to focus on what's working and defer Lazy resolution to a dedicated session.

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
