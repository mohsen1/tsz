# Session tsz-4: Control Flow Analysis & Narrowing Refinement

**Started**: 2026-02-04
**Goal**: Refine Control Flow Analysis (CFA) to reduce false positives and improve soundness

## Problem Statement

TypeScript's type system relies on precise control flow analysis and type narrowing to correctly:
1. Narrow union types based on type guards (`typeof`, `instanceof`, discriminants)
2. Track definite assignment of variables
3. Detect when properties exist on narrowed types

Current issues:
- **TS2339 High Extra Errors**: "Property does not exist" false positives when property exists on narrowed type
- **TS2454 Missing Errors**: Variables used before assignment not being detected in all cases
- **52 Pre-existing Test Failures**: Many likely related to missing or incorrect narrowing logic

## Scope

Refine control flow analysis and narrowing in three areas:

### 1. Union Type Narrowing
- Discriminant narrowing (tagged unions)
- `typeof` narrowing
- `instanceof` narrowing
- Truthiness narrowing
- Assignment narrowing

### 2. Definite Assignment Analysis
- Track variable assignments through control flow
- Detect use-before-assignment in all branches
- Handle nested scopes and closures

### 3. Property Existence Checking
- Use narrowed types for property access
- Reduce TS2339 false positives
- Distinguish between "definitely missing" and "might be missing"

## Target Files

- `src/checker/control_flow.rs` - Flow graph traversal
- `src/checker/control_flow_narrowing.rs` - Narrowing predicates
- `src/checker/flow_analysis.rs` - Definite assignment
- `src/checker/flow_analyzer.rs` - Forward dataflow analysis

## Implementation Strategy

### Phase 1: Audit Current State
- Identify specific test failures related to narrowing
- Document current narrowing behavior vs tsc
- Measure baseline TDZ and narrowing accuracy

### Phase 2: Fix High-Impact Issues
- Focus on narrowing patterns that affect many tests
- Discriminant narrowing (if statements, switch statements)
- Type guard narrowing (`typeof`, `instanceof`)

### Phase 3: Refine Definite Assignment
- Improve flow-sensitive assignment tracking
- Handle complex control flow (loops, try/catch, early returns)
- Fix TDZ detection edge cases

## Success Criteria

- [ ] Reduce TS2339 extra errors by 20%+
- [ ] Increase TS2454 missing errors by 10%+
- [ ] Fix 10+ of the 52 pre-existing test failures
- [ ] All changes tested, committed, and pushed
- [ ] No regressions in previously passing tests

## Progress

### 2025-02-04: Initial Investigation

**Test Case Being Debugged**:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
declare function fn(): D;
let o: D;
if ((o = fn()).done) {
    const y: 1 = o.value;  // Should work - o should be narrowed to { done: true, value: 1 }
}
```

**Expected (tsc)**: No errors
**Actual (tsz)**: TS2322 - Type '1 | 2' is not assignable to type '1'

**Investigation Findings**:
- Discriminant narrowing IS implemented in `src/solver/narrowing.rs`
- Functions: `find_discriminants()`, `narrow_by_discriminant()`, `narrow_by_excluding_discriminant()`
- Control flow integration in `src/checker/control_flow.rs` (lines 1607-1617)
- TypeGuard extraction exists but marked as `#[allow(dead_code)]`

**Root Cause Hypothesis**:
The issue is NOT with discriminant narrowing logic itself, but with integration:
1. Assignment in condition: `o = fn()` happens in the if condition
2. Discriminant check: `.done` is checked on the assigned variable
3. Narrowing doesn't propagate: The narrowed type from the condition isn't properly propagated to the if body for variables assigned in the condition

**Key Files**:
- `src/solver/narrowing.rs` - Core narrowing logic (complete)
- `src/checker/control_flow.rs` - Integration point (needs fix)
- `src/checker/control_flow_narrowing.rs` - TypeGuard extraction (dead code, needs activation)

**Next Steps**:
1. Fix assignment + discriminant combo in control flow analysis
2. Ensure narrowed types propagate correctly to statement bodies
3. Test with `test_assignment_expression_condition_narrows_discriminant`

## Notes

- Builds on TDZ work from tsz-2
- Addresses documented gaps in `docs/walkthrough/04-checker.md`
- Focuses on soundness (missing errors) and precision (extra errors)
- High value for improving conformance and user experience

## Related Sessions

- tsz-1: Parser fixes (COMPLETED)
- tsz-2: TDZ checking (COMPLETED)
- tsz-3: Scope-aware symbol merging (COMPLETED)
