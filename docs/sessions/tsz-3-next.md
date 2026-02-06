# Session tsz-3: Phase 4.2 Cleanup / Anti-Pattern 8.1 Refactoring

**Started**: 2026-02-06
**Status**: Active - Planning Phase
**Predecessor**: tsz-3-infer-fix-complete (Conditional Type Inference - COMPLETED)

## Completed Sessions

1. **In operator narrowing** - Filtering NEVER types from unions in control flow narrowing
2. **TS2339 string literal property access** - Implementing visitor pattern for primitive types
3. **Conditional type inference with `infer` keywords** - Fixed `collect_infer_type_parameters_inner` to recursively check nested types

## Current Task: Phase 4.2 Cleanup / Anti-Pattern 8.1 Refactoring

### Task Definition (from Gemini Consultation)

Based on review of NORTH_STAR.md and docs/sessions/, the next priority task is:

**Remove Anti-Pattern 8.1: Checker matching on TypeKey**

From NORTH_STAR.md Section 8.1:
> "Checker components must NOT directly pattern-match on TypeKey. This creates tight coupling
> between Checker and Solver implementation details, violating architectural boundaries."

### Problem

The Checker currently has code that directly matches on `TypeKey` enum variants, which:
1. Creates tight coupling between Checker and Solver
2. Violates the Solver-First architecture
3. Makes it harder to modify Solver internals without breaking Checker

### Files to Investigate

Per Gemini's recommendation:
1. **`src/checker/assignability_checker.rs`** - Line 55: `ensure_refs_resolved_inner` has mixed `Lazy` and `TypeQuery(SymbolRef)` logic
2. **`src/checker/flow_analysis.rs`** - May have direct TypeKey matching

### Implementation Approach (Pending Gemini Review)

**Before implementing, I will ask Gemini Question 1**:
1. Is this the right approach?
2. What functions should be modified?
3. What are the edge cases?

**Planned Approach**:
1. Identify all places in Checker that directly match on TypeKey
2. Replace with calls to `self.with_judge(|j| j.classify_...)` or new visitor-based queries
3. Create new type query methods in `src/solver/type_queries.rs` if needed
4. Update session file with progress

### Test Cases

**Before**: Existing tests should pass (no behavior change, just refactoring)
**After**: All tests should still pass, with improved architectural separation

## Previous Session Details (Archived)

### Conditional Type Inference (COMPLETED)

**Problem**: `infer R` in conditional types caused "Cannot find name 'R'" errors

**Root Cause**: `collect_infer_type_parameters_inner` in `type_checking_queries.rs` did not check for InferType nodes inside nested type structures

**Solution**: Added recursive checking for InferType in 13 different type patterns:
- Function/Constructor types (parameters and return type)
- Array types (element type)
- Tuple types (all elements)
- Type literals (all members)
- Type operators (keyof, readonly, unique)
- Indexed access types (object and index)
- Mapped types (type parameter constraint, type template)
- Conditional types (all branches)
- Template literal types (spans)
- Parenthesized, optional, rest types
- Named tuple members
- Parameters (type annotations)
- Type Parameters (constraint and default)

**Committed**:
- `2c238b893`: feat(checker): fix infer type collection in nested types
- `4eab170d1`: fix(checker): add TYPE_PARAMETER handling for infer collection

**Result**: Redux test "Cannot find name" errors eliminated, conformance tests passing
