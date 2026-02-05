# Session TSZ-3: Fix Narrowing Regressions & TS2339 Property Access

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix 3 failing narrowing tests from commit 73ac6b1a8, then tackle narrowing-related TS2339 errors

## Problem Statement

**Immediate Issue**: Commit `73ac6b1a8` (transitive re-export support) introduced 3 failing tests:
1. `test_array_destructuring_assignment_clears_narrowing`
2. `test_truthiness_false_branch_narrows_to_falsy`
3. `test_array_destructuring_default_initializer_clears_narrowing`

All show `TypeId` mismatches in narrowing logic.

**Broader Impact**: These regressions block other agents (like tsz-18) from accurately measuring conformance progress. TS2339 is the #1 source of false positives (621 errors), many caused by narrowing failures.

## Success Criteria

### Phase A: Fix Regressions
- [ ] All 3 failing tests pass
- [ ] No new test failures introduced
- [ ] Understand root cause of each failure

### Phase B: Reduce TS2339 False Positives
- [ ] Fix discriminant narrowing with Lazy types
- [ ] Fix truthiness narrowing for unknown
- [ ] Measure reduction in TS2339 errors

## Implementation Plan

### Phase A1: Fix `test_truthiness_false_branch_narrows_to_falsy`

**File**: `src/solver/narrowing.rs`
**Function**: `narrow_to_falsy`

**Issue**: TypeScript behavior is subtle - `boolean` narrows to `false`, but `string` and `number` often stay as the primitive in the false branch unless they were already literal unions.

**Action**:
1. Run test with tracing: `TSZ_LOG="wasm::solver::narrowing=trace" cargo test test_truthiness_false_branch_narrows_to_falsy -- --nocapture`
2. Compare `narrow_to_falsy` implementation against `tsc`'s `getFalsyType`
3. Ask Gemini for approach validation if unclear
4. Implement fix
5. Ask Gemini for review

### Phase A2: Fix Destructuring Failures (Tests 1 & 3)

**Files**: `src/checker/flow_graph_builder.rs`, `src/checker/control_flow.rs`
**Functions**: `handle_expression_for_assignments`, `assignment_targets_reference_node`

**Issue**: Commit `73ac6b1a8` might have broken recursive detection of assignment targets in `ARRAY_LITERAL_EXPRESSION`. If flow graph doesn't see the assignment, it won't create the `ASSIGNMENT` node that kills previous narrowing.

**Action**:
1. Run tests with tracing: `TSZ_LOG="wasm::checker::control_flow=trace" cargo test test_array_destructuring_assignment_clears_narrowing -- --nocapture`
2. Verify `is_assignment_operator_token` correctly identifies assignment
3. Verify `handle_expression_for_assignments` recurses into array patterns
4. Ask Gemini for approach validation if unclear
5. Implement fix
6. Ask Gemini for review

### Phase B: Narrowing-Related TS2339

**Goal**: Fix discriminant and truthiness narrowing to reduce property access false positives.

**Focus Areas**:
1. Discriminant narrowing with Lazy types in `narrow_by_discriminant`
2. Truthiness narrowing for `unknown` in `narrow_by_truthiness`

## MANDATORY Gemini Workflow

Per AGENTS.md and CLAUDE.md, **MUST ask Gemini TWO questions** for any solver/checker changes:

### Question 1 (PRE-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --include=src/solver/narrowing "
I am fixing test_truthiness_false_branch_narrows_to_falsy.
Problem: [DESCRIBE THE ISSUE]
My planned approach: [YOUR PLAN]

Before I implement: 1) Is this the right approach? 2) What functions should I modify?
3) What edge cases do I need to handle?
"
```

### Question 2 (POST-implementation) - REQUIRED
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing "
I fixed [FEATURE] in [FILE].
Changes: [PASTE CODE OR DESCRIBE CHANGES]

Please review: 1) Is this correct for TypeScript? 2) Did I miss any edge cases?
3) Are there type system bugs?
"
```

## Dependencies

- **tsz-3 previous sessions**: Discriminant narrowing, CFA - expertise in this area
- **tsz-18**: Conformance testing - will benefit from these fixes

## Related Sessions

- **tsz-1**: Discriminant Narrowing (COMPLETE) - foundational work
- **tsz-18**: Conformance Testing (IN PROGRESS) - blocked by these regressions

## Session History

**Created 2026-02-05** following Gemini consultation to define new tsz-3 session after previous completion.

Previous tsz-3 sessions:
- Discriminant narrowing investigation (COMPLETE)
- Control flow analysis (COMPLETE)
- Declaration emit (COMPLETE)

## Progress

### 2026-02-05: Session Created
- Consulted Gemini for session definition
- Received clear recommendation to fix narrowing regressions
- Created session file
- Ready to start Phase A1
