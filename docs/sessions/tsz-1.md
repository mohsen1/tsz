# TSZ-1 Session Log

**Session ID**: tsz-1
**Last Updated**: 2025-02-05
**Focus**: Solver (Type Relations & Narrowing)

## Active Tasks

### Task #13: Type Narrowing (Truthiness & typeof)
**Status**: Pending
**Priority**: High (foundational for Control Flow Analysis)
**Estimated Impact**: +3-5% conformance

**Description**:
Implement type narrowing in `src/solver/narrowing.rs`:
- Truthiness narrowing: `if (x) { ... }` removes `null`/`undefined` from types
- `typeof` narrowing: `if (typeof x === 'string')` narrows to string type
- Build on CFG/FlowNode infrastructure from reachability analysis

**Gemini Guidance**:
> "Reachability tells you *if* a node is reached; Narrowing tells you *what* the types are when it is reached."
>
> "Before implementing, ask for approach validation on how to link FlowNode data to the Solver's narrowing queries."

**Prerequisites**:
- Follow Two-Question Rule (ask Gemini BEFORE implementing)
- Understand FlowNode → Solver integration pattern

## Completed Tasks

### Task #11: Method/Constructor Overload Validation
**Status**: ✅ Completed
**Date**: 2025-02-05
**Implementation**: Added manual signature lowering infrastructure in `src/solver/lower.rs`
**Result**: TS2394 now works for methods and constructors

### Task #12: Reachability Analysis (TS7027)
**Status**: ✅ Completed
**Date**: 2025-02-05
**Finding**: Already implemented in `src/checker/reachability_checker.rs`
**Verification**: Tested with unreachable code scenarios - all working correctly

## Quick Wins (Backlog)

### Excess Property Checking (TS2353)
**Priority**: Medium (+1-2% conformance)
**Location**: `src/solver/lawyer.rs` or `src/solver/compat.rs`
**Description**: Implement check for extra properties in object literals

### Optional Property Subtyping Fixes
**Priority**: Medium
**Location**: `src/solver/subtype.rs`
**Description**: Fix logic for `{ prop?: "a" }` cases with optional properties and undefined

## Session Direction

**Current Focus**: Solver work (Type Relations & Narrowing)
- **Why**: Solver is the "WHAT" - defines type relationships and narrowing logic
- **Goal**: Build robust, complete type system operations

**Key Principles** (from AGENTS.md):
1. **Two-Question Rule**: Always ask Gemini BEFORE and AFTER implementing solver/checker changes
2. **Type Resolution**: Every relation check must handle Lazy, Ref, and Intersection types
3. **Directionality**: Ensure correct subtype check ordering (literal <: property_type, not reverse)

**Recent Learning** (from AGENTS.md investigation 2026-02-04):
- Even "working" features like discriminant narrowing had critical bugs
- 100% of unreviewed implementations had type system bugs
- Gemini Pro consultation is NON-NEGOTIABLE for solver/checker changes

## Recent Commits

- `f78fd2493`: docs(tsz-9): record Gemini Pro approval - plan validated
- `7353a8310`: docs(tsz-9): document investigation findings and bug report
