# Session TSZ-3 Phase C: Narrowing-Aware Property Access

**Started**: 2026-02-05
**Status**: 🔄 READY TO START
**Focus**: Fix property access resolution to use narrowed types, reducing TS2339 false positives

## Problem Statement

**TS2339 False Positives**: 621 errors for "Property 'x' does not exist on type 'y'"

**Root Cause Hypothesis**: The checker may not be consulting flow-narrowed types when resolving property access, or narrowing logic has bugs in specific operators (`in`, `instanceof`, discriminant properties).

**Immediate Issues**:
1. 4 failing `in` operator narrowing tests (pre-existing)
2. Potential discriminant narrowing bugs (per AGENTS.md warning about previous attempt having 3 critical bugs)

## Success Criteria

### Phase C1: Fix Operator Narrowing Tests
- [ ] Fix `test_in_operator_narrows_required_property`
- [ ] Fix `test_in_operator_optional_property_keeps_false_branch_union`
- [ ] Fix `test_in_operator_private_identifier_narrows_required_property`
- [ ] Fix `test_instanceof_narrows_to_object_union_members`

### Phase C2: Discriminant Property Narrowing
- [ ] Fix reversed subtype check (use `is_subtype_of(literal, property_type)`)
- [ ] Handle Lazy/Ref/Intersection type resolution
- [ ] Handle optional properties correctly

### Phase C3: Checker Integration
- [ ] Verify `check_property_access_expression` uses flow-narrowed types
- [ ] Measure reduction in TS2339 errors (goal: 20% reduction = 120+ errors)

## Implementation Plan

### Step 1: Investigate Failing Tests
Run each failing test with tracing to understand the narrowing failure point:
```bash
TSZ_LOG="wasm::solver::narrowing=trace" cargo test test_in_operator_narrows_required_property -- --nocapture
```

### Step 2: Ask Gemini PRE-Implementation
```bash
./scripts/ask-gemini.mjs --include=src/solver --include=src/checker "I need to fix 'in' operator narrowing.
Problem: 'a' in x doesn't narrow x: { a: number } | { b: string } to { a: number }
My planned approach: [YOUR PLAN]

Before I implement: 1) Is this the right approach? 2) What functions should I modify?
3) What edge cases do I need to handle?"
```

### Step 3: Implement & Test
- Fix narrowing logic in `src/solver/narrowing.rs`
- Run tests to verify
- Ask Gemini POST-implementation review

### Step 4: Discriminant Narrowing (if applicable)
- Fix subtype check direction
- Add type resolution for Lazy/Ref/Intersection
- Handle optional properties

## Files to Investigate

- `src/solver/narrowing.rs` - Operator narrowing logic
- `src/checker/expr.rs` - Property access expression checking
- `src/solver/visitor.rs` - Type resolution visitors

## Dependencies

- **TSZ-3 Phases A & B**: Completed compound assignment and array mutation narrowing
- **AGENTS.md**: Warning about previous discriminant narrowing bugs

## Related Sessions

- **TSZ-3 (previous)**: CFA Completeness - Phases A & B (COMPLETE)

## Progress

### 2026-02-05: Session Created
- Phases A and B completed successfully
- 4 pre-existing failing tests identified
- Session plan created with clear success criteria

---

*Session created by tsz-3 on 2026-02-05*
