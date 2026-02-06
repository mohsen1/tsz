# Session tsz-3: Object Literal Freshness - Architectural Solution

**Started**: 2026-02-06
**Status**: 🔄 NEW APPROACH - Lawyer/Judge Pattern
**Predecessor**: Index Access Type Evaluation (Already Implemented)

## Problem Summary

Object literal freshness widening is not working correctly. Initial investigation revealed a cache poisoning issue where `widen_freshness` creates a new TypeId, but `node_types` cache returns the original fresh TypeId.

**Previous attempt**: Fixing cache mutations in `check_variable_declaration` - Failed due to complex node caching behavior.

## New Approach: Lawyer/Judge Pattern

Gemini suggested moving freshness handling to the **Lawyer** layer (compatibility checker) rather than fighting the cache in the Checker.

### Key Insight

From `docs/architecture/NORTH_STAR.md`:
- **Judge** (`src/solver/subtype.rs`): Pure structural subtyping, ignores freshness
- **Lawyer** (`src/solver/lawyer.rs` or `src/solver/compat.rs`): Handles TypeScript-specific quirks like freshness

### Architectural Solution

1. **Keep FRESH_LITERAL flag** on types in `node_types` cache
2. **Don't widen** during variable declaration
3. **Let the Lawyer decide** when freshness matters during assignability checks

### Implementation Plan

1. **In `src/solver/compat.rs`** (The Lawyer):
   - Modify `check_assignability` or excess property check logic
   - When checking if a FRESH type is assignable, perform excess property check
   - When checking if a NON-FRESH type is assignable, skip excess property check

2. **In `src/checker/`**:
   - Call Lawyer's `is_assignable_to` instead of raw `is_subtype_of`
   - Pass context flags (direct vs. indirect assignment)

3. **Widening logic**:
   - Should the Lawyer handle widening?
   - Or should the Checker request a widened type from the Solver?

## Next Steps

Ask Gemini the Two-Question Rule before implementing:
1. **Question 1** (Pre-implementation): Validate the Lawyer approach, get specific file/functions to modify
2. **Question 2** (Post-implementation): Review the implementation for correctness

## Files to Investigate

- `src/solver/compat.rs` - Main Lawyer compatibility layer (has `find_excess_property`)
- `src/solver/subtype.rs` - Judge (pure subtyping)
- `src/checker/assignability_checker.rs` - Checker's assignability calls
- `docs/architecture/NORTH_STAR.md` - Section 3.3 on Judge vs Lawyer

## Test Files

- `tests/conformance/expressions/objectLiterals/excessPropertyChecking.ts`
- `tests/conformance/types/objectLiterals/freshness/`
- `src/checker/tests/freshness_stripping_tests.rs` (6 failing tests)
