# Session TSZ-11: Readonly Type Support

**Started**: 2026-02-06
**Status**: 🔄 IN PROGRESS
**Predecessor**: TSZ-10 (Flow Narrowing - Deferred)

## Task

Implement proper subtyping and assignability logic for the `ReadonlyType` wrapper type.

## Problem Statement

TypeScript's `readonly` modifier creates a subtype relationship where:
- `readonly T[]` is assignable to `T[]` (readonly can be assigned to mutable)
- `T[]` is NOT assignable to `readonly T[]` (mutable cannot be assigned to readonly)
- Same applies to readonly object properties and index signatures

The `TypeKey::ReadonlyType(T)` wrapper exists but proper subtype checking is not implemented.

## Expected Impact

- **Direct**: Fix ~3 readonly-specific conformance tests
- **Tests**:
  - `test_readonly_array_element_assignment_2540`
  - `test_readonly_element_access_assignment_2540`
  - `test_readonly_index_signature_element_access_assignment_2540`

## Files to Modify

1. **src/solver/subtype.rs** - Update `solve_subtype` to handle `TypeKey::ReadonlyType(T)`
2. **src/solver/visitor.rs** - Update visitors to unwrap `ReadonlyType` when needed
3. **src/solver/lawyer.rs** - Update Lawyer compatibility rules for readonly

## Implementation Plan (TO BE VALIDATED BY GEMINI)

### Phase 1: Ask Gemini for Approach Validation

Before implementing, ask Gemini:
1. Is this the correct approach for readonly subtyping?
2. What exact functions need to be modified?
3. Are there TypeScript-specific behaviors to match?
4. Are there edge cases (e.g., nested readonly, readonly in unions)?

### Phase 2: Implement

Based on Gemini's guidance, implement the subtype logic.

### Phase 3: Test and Commit

Run conformance tests and commit if passing.

## Test Status

**Start**: 8232 passing, 68 failing

## Notes

Following AGENTS.md mandatory workflow:
- Question 1 (Pre-implementation): Ask Gemini for approach validation
- Question 2 (Post-implementation): Ask Gemini for implementation review
