# Session TSZ-3: CFA Completeness & TS2339 Resolution

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix compound assignments, array mutation side-effects, and improve property access integration

## Problem Statement

**Immediate Issues**: Two pre-existing control flow analysis (CFA) tests failing:
1. `test_compound_assignment_clears_narrowing` - Compound assignments (`+=`, `-=`) don't properly narrow to result type
2. `test_array_mutation_clears_predicate_narrowing` - Array mutations don't properly clear predicate narrowing

**Broader Impact**: TS2339 remains the #1 source of false positives (621 errors). While narrowing logic is correct, property access resolution may not be properly consulting narrowed types.

## Success Criteria

### Phase A: Fix Compound Assignments
- [x] `test_compound_assignment_clears_narrowing` passes
- [x] `get_assigned_type` handles compound operators
- [x] Compound assignments properly kill narrowing and narrow to result type

### Phase B: Fix Array Mutation Side-Effects
- [ ] `test_array_mutation_clears_predicate_narrowing` passes
- [ ] Array mutations (`push`, `pop`, etc.) properly clear narrowed types
- [ ] Flow graph correctly tracks mutation side-effects

### Phase C: Property Access Integration (Optional)
- [ ] Investigate TS2339 false positives
- [ ] Verify property access consults narrowed types
- [ ] Measure reduction in TS2339 errors

## Implementation Plan

### Phase A: Compound Assignments

**Issue**: Currently `x += 1` doesn't trigger proper narrowing. The `get_assigned_type` function doesn't handle compound operators.

**Files to Modify**: `src/checker/control_flow_narrowing.rs`, `src/checker/flow_graph_builder.rs`

**Approach** (consult Gemini before implementing):
1. Ensure compound assignments are added to flow graph as ASSIGNMENT nodes
2. Update `get_assigned_type` to handle compound operators (`+=`, `-=`, `*=`, etc.)
3. For `+=`: Result type depends on operand type (string concatenation vs number addition)
4. For other operators: Result is typically the primitive type (number for `-=`, `*=`, etc.)

**Action**:
1. Ask Gemini: "How should I implement narrowing clearing for compound assignments?"
2. Implement fix based on guidance
3. Ask Gemini: "Review my compound assignment implementation"
4. Test and commit

### Phase B: Array Mutations

**Issue**: When `x.push(...)` is called on a narrowed array type, the narrowing should be cleared because the array contents may have changed.

**Files to Modify**: `src/checker/control_flow.rs`

**Approach** (consult Gemini before implementing):
1. Check if ARRAY_MUTATION flag is being properly set
2. Ensure array mutations kill predicate-based narrowing
3. May need to track mutation side-effects on narrowed symbols

**Action**:
1. Ask Gemini: "How should array mutations clear predicate narrowing?"
2. Implement fix based on guidance
3. Ask Gemini: "Review my array mutation implementation"
4. Test and commit

### Phase C: Property Access Integration

**Investigation Needed**: Trace a failing TS2339 test to see if property access is consulting narrowed types.

**Files to Investigate**: `src/checker/type_computation.rs`, `src/checker/expr.rs`

**Action**:
1. Find a minimal TS2339 false positive case
2. Trace through `get_type_of_property_access`
3. Check if `get_type_of_node` is consulting flow analysis
4. If not, add flow analysis consultation
5. Measure impact on TS2339 false positive count

## MANDATORY Gemini Workflow

Per AGENTS.md and CLAUDE.md, **MUST ask Gemini TWO questions** for any solver/checker changes:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --include=src/checker "I need to implement compound assignment narrowing.
Problem: x += 1 doesn't properly narrow x to number
My planned approach: [YOUR PLAN]

Before I implement: 1) Is this the right approach? 2) What functions should I modify?
3) What edge cases do I need to handle (e.g., string concatenation vs number addition)?
"
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker "I implemented compound assignment narrowing.
Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review: 1) Is this correct for TypeScript? 2) Did I miss any edge cases?
3) Are there type system bugs?
"
```

## Dependencies

- **tsz-3 previous session**: Completed destructuring and any propagation fixes
- **tsz-18**: Conformance testing - will benefit from these fixes

## Related Sessions

- **tsz-1**: Discriminant Narrowing (COMPLETE) - foundational work
- **tsz-3 (previous)**: Fix Narrowing Regressions & Any Propagation (COMPLETE) - immediate predecessor

## Session History

**Created 2026-02-05** following successful completion of previous tsz-3 session.

Previous tsz-3 sessions:
- Fix Narrowing Regressions & TS2339 Property Access (COMPLETE)
- Discriminant narrowing investigation (COMPLETE)
- Control flow analysis (COMPLETE)
- Declaration emit (COMPLETE)

## Progress

### 2026-02-05: Session Initiated
- Marked previous session as complete
- Consulted Gemini for session definition
- Identified 2 pre-existing failing tests as focus areas
- Created session file with clear success criteria

### 2026-02-05: Phase A Complete - Compound Assignment Fix

**Status**: ✅ Compound assignment narrowing implemented and tested

**What Was Done**:
1. Consulted Gemini Flash for approach validation
2. Implemented compound assignment handling in `get_assigned_type`
3. Initially used simple fallback (NUMBER for all arithmetic operators)
4. Gemini Pro review revealed bug: += could be string concatenation OR number addition
5. Refined implementation to use literal type checking for += heuristic
6. Added helper functions: is_compound_assignment_operator, map_compound_operator_to_binary, is_number_type

**Implementation Details**:
- **File**: `src/checker/control_flow.rs`
- **Function**: `get_assigned_type` (lines 1274-1370)
- **Key Changes**:
  - Detects compound assignment operators (+=, -=, *=, etc.)
  - When `node_types` available: Uses `BinaryOpEvaluator` to compute result type
  - When `node_types` unavailable: Uses heuristics
    - Arithmetic/bitwise operators → NUMBER
    - `+=` with numeric literal → NUMBER
    - `+=` with non-literal → preserve narrowing (could be string)
    - Logical/??= operators → preserve narrowing

**Test Results**:
- ✅ `test_compound_assignment_clears_narrowing` - PASSING
- ⚠️ `test_array_mutation_clears_predicate_narrowing` - Still failing (pre-existing issue)

**Commit**: c992f94c9 - "feat(flow-analysis): add compound assignment narrowing"

---

*Session updated by tsz-3 on 2026-02-05*
