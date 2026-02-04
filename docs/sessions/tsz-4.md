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

### 2025-02-04: Fixed False Positive TS2564 in Switch Statements ✅

**Problem**: `analyze_switch_statement` incorrectly returned `normal: None` when there was no default clause, marking all subsequent code as unreachable. This caused false positive TS2564 errors for properties initialized after switch statements without default clauses.

**Example Bug**:
```typescript
class C {
    prop: number; // FALSE POSITIVE TS2564 before fix
    constructor(value: number) {
        switch (value) {
            case 1:
                this.prop = 10;
                break;
            // No default clause
        }
        // Bug: Code after switch was marked unreachable
        this.prop = 30; // Property IS definitely assigned here
    }
}
```

**Root Cause**:
In `src/checker/flow_analysis.rs:518-525`, when `!has_default_clause`:
- **Before**: Returned `normal: None` → marks subsequent code unreachable
- **After**: Returns `normal: Some(assigned)` → preserves flow continuation

**Fix**:
```rust
if !has_default_clause {
    // Without a default, we can't guarantee any case will execute
    // However, execution CAN continue after the switch (fall-through)
    // Return the incoming assignments to preserve the normal flow
    return FlowResult {
        normal: Some(assigned),  // Changed from: None
        exits,                    // Changed from: Some(assigned.clone())
    };
}
```

**Testing**:
- Created test cases to verify fix with `--strict` mode
- Verified against tsc output
- Confirmed properties initialized after switch without default are now correctly recognized
- Confirmed properties NOT initialized in all paths still correctly emit TS2564

**Commit**: `efb5d0807` - "fix: correct flow analysis for switch statements without default clauses"

**Impact**:
- Eliminates false positive TS2564 errors for properties initialized after switch statements
- Improves flow analysis accuracy for switch statements without default clauses
- No test regressions introduced

**Remaining Work**:
- TS2454 (variable TDZ) still not being reported for variables after switch - separate issue

### 2025-02-04: Next Task - Discriminant Narrowing for Assignment Expressions

**Current Issue**: Test `test_assignment_expression_condition_narrows_discriminant` failing

**Problem Pattern**:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
declare function fn(): D;
let o: D;
if ((o = fn()).done) {
    const y: 1 = o.value;  // Should narrow o to { done: true, value: 1 }
}
```

**Expected (tsc)**: No errors - `o` is narrowed to `{ done: true, value: 1 }`
**Actual (tsz)**: TS2322 - Type '1 | 2' is not assignable to type '1'

**Root Cause**:
- Discriminant narrowing logic exists in `src/solver/narrowing.rs` ✅
- Issue is AST traversal/integration in checker - doesn't unwrap `(Assignment).prop` pattern
- Need to look through `ParenthesizedExpression` and `AssignmentExpression`

**Implementation Steps**:
1. Locate condition analysis in `src/checker/control_flow.rs`
2. Add unwrapping logic for assignment expressions in conditions
3. Extract target variable from `(x = expr).prop` pattern
4. Apply narrowing to the extracted variable

**Test Command**:
```bash
cargo nextest run test_assignment_expression_condition_narrows_discriminant
```

**Impact**: High - common pattern in iterators (`while (!(res = iter.next()).done)`) and result handling

### 2025-02-04: Discovery - Basic Discriminant Narrowing Not Working

**Investigation**:
Created test case for simple discriminant narrowing (without assignment):
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
let o: D = { done: true, value: 1 };
if (o.done) {
    const y: 1 = o.value;  // Should narrow to { done: true, value: 1 }
}
```

**Finding**: Even basic discriminant narrowing (without assignment expressions) is not working!
- **tsc**: No errors (correct narrowing)
- **tsz**: TS2322 error (narrowing not applied)

**Root Cause**: The issue is NOT with the integration point I fixed, but with the discriminant narrowing infrastructure itself. The narrowing logic in `narrow_by_discriminant` may not be correctly integrated with the flow analysis.

**Next Steps**: Need to investigate why discriminant narrowing is not being applied even for simple cases before the assignment expression fix can be properly tested.

**Commit**: `8fdd91417` - Added assignment unwrapping logic (test still reveals deeper issue)

### Session Status: PAUSED

**Summary of Work**:
1. ✅ Fixed false positive TS2564 in switch statements without default clauses
2. ⚠️ Started discriminant narrowing for assignment expressions - discovered deeper infrastructure issue

**Discovery**:
Basic discriminant narrowing (even without assignments) is not working in tsz. The solver logic exists in `src/solver/narrowing.rs` but the integration with flow analysis is broken.

**Evidence**:
```typescript
type D = { done: true, value: 1 } | { done: false, value: 2 };
let o: D = { done: true, value: 1 };
if (o.done) {
    const y: 1 = o.value;  // tsc: ok, tsz: TS2322
}
```

**Root Cause Analysis**:
The discriminant narrowing infrastructure requires broader investigation:
- `narrow_by_discriminant` in solver is implemented and tested
- Flow analysis calls `discriminant_property` to find discriminant patterns
- Integration point exists but narrowing is not being applied to variable types
- May require changes to how narrowed types flow from conditions to variable bindings

**Recommendation**:
This task is too complex for a single session. Requires:
1. Deep dive into flow analysis → narrowed type propagation
2. Possibly restructuring how narrowing is applied
3. Testing across multiple narrowing patterns

**Next Steps**:
- Choose a different, more achievable CFA task for this session
- Or defer discriminant narrowing to a dedicated multi-session effort
- Focus on simpler flow analysis fixes with clear scope

## Notes

- Builds on TDZ work from tsz-2
- Addresses documented gaps in `docs/walkthrough/04-checker.md`
- Focuses on soundness (missing errors) and precision (extra errors)
- High value for improving conformance and user experience

## Related Sessions

- tsz-1: Parser fixes (COMPLETED)
- tsz-2: TDZ checking (COMPLETED)
- tsz-3: Scope-aware symbol merging (COMPLETED)
