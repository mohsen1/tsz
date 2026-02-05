# Session tsz-3: CFA Refinement - Nested Discriminants & Any-Safety

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
**Previous Session**: tsz-3 (CFA Features - Complete)

## Goal

Unblock critical architectural issues in Control Flow Analysis that prevented completion of advanced features.

## Context from Previous tsz-3

The previous session successfully delivered:
- ✅ Phase 1: Bidirectional Narrowing (x === y narrowing)
- ✅ Phase 2: Assertion Functions (asserts x is T)

But encountered blocking issues:
- ⏸️ Phase 3: Nested Discriminants - Required AccessPath abstraction
- ⏸️ Phase 4.1: Any Narrowing - Broke circular extends tests

## Progress

### Phase 1: Nested Discriminant Architecture (🔄 ACTIVE)

**Status**: 🟡 IN PROGRESS - ARCHITECTURAL INVESTIGATION

**Problem**: Support narrowing for nested discriminant paths like `action.payload.kind`.

**Root Cause** (from previous session):
- Current narrowing only tracks top-level identifiers
- Narrowing `x.y.kind === "a"` requires Checker to communicate path to Solver
- The `is_matching_reference(base, target)` check prevents nested narrowing

**Architectural Challenge**:
- **Solver-First Architecture** (Section 3.1): Checker provides "Where" (path) and "What" (literal), Solver performs narrowing
- Need to update Solver's `narrow()` interface to accept `Path` (Vec<Atom>) alongside TypeId
- Must handle optional chaining (obj?.prop.kind) correctly

**Implementation Plan**:
1. Investigate how to integrate PropertyPath into narrowing logic
2. Refactor Solver to support path-based narrowing
3. Update Checker's flow analysis to track access expressions
4. Ensure backward compatibility with existing identifier narrowing

**Gemini Consultation**: Pending - Must use Two-Question Rule before implementation

---

### Phase 2: Any-Narrowing & Circularity (⏸️ PENDING)

**Status**: ⏸️ NOT STARTED

**Problem**: Narrowing logic involving `any` triggers circularity in `is_subtype_of`.

**Root Cause**:
- Previous attempt to narrow `any` for typeof checks broke 5 circular extends tests
- Likely violates type system invariants in recursive type resolution

**Architectural Challenge**:
- **Judge vs. Lawyer** (Section 3.3): Need to apply AnyPropagationRules from `src/solver/lawyer.rs`
- `any` should not trigger strict subtype checking that leads to cycles
- Must handle `any` as a special case that narrows "silently"

**Implementation Plan**:
1. Investigate circular extends errors from previous attempt
2. Implement Lawyer layer integration for any narrowing
3. Add cycle guards to prevent infinite recursion
4. Test with comprehensive circular type scenarios

---

## Session Notes

This session focuses on unblocking critical architectural issues in the CFA engine. Both features are high-priority "North Star" requirements that prevent tsz from matching TypeScript's behavior in real-world scenarios.

**Key Principle**: Follow Two-Question Rule strictly for ALL solver/checker changes.
