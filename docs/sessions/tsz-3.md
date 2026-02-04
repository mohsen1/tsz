# Session tsz-3: Narrowing Logic Correctness

**Started**: 2026-02-04
**Status**: ACTIVE
**Focus**: Implement robust, tsc-compliant narrowing logic to fix 8+ critical bugs

## Context

Transitioning from **review phase** (COMPLETE) to **implementation phase**.
Review findings archived in `docs/sessions/history/tsz-3-review-20260204.md`.

## Problem Statement

The narrowing logic in `src/solver/narrowing.rs` has 8+ critical bugs that cause incorrect type narrowing in control flow analysis. These bugs affect discriminant narrowing, instanceof, and the `in` operator.

## Tasks

### Task 1: Discriminant Narrowing Fix
**Function**: `narrow_by_discriminant`
**Bugs**: 3 (reversed check, no resolution, optional props)

**Implementation**:
1. Use **filtering approach** - not pre-discovery
2. For each union member, use `resolve_property_access` to handle Lazy/Intersection/Apparent
3. Check `is_subtype_of(literal, property_type)` - NOT reversed
4. Handle optional properties: `{ prop?: "a" }` accounts for implicit `undefined`

**Reference**: Gemini Question 1 response from review phase

**Status**: ⏸️ Plan ready, pending Question 1 validation

---

### Task 2: `in` Operator Narrowing Fix
**Function**: `narrow_by_property_presence`, `type_has_property`
**Bugs**: 4+ (unknown, optional, resolution, intersection)

**Progress**: 2 of 4 fixes complete (commit 68c367e2b)

**Completed** ✅:
1. **unknown handling**: Narrows to `object & { prop: unknown }` in true branch
2. **Intersection support**: Checks all intersection members, returns true if ANY has property

**Remaining** ⏸️:
3. **Lazy type resolution**: Requires adding resolver to NarrowingContext (architectural change)
4. **Optional property promotion**: Requires ObjectShape cloning and re-interning
5. **Prototype property checking**: Requires apparent.rs integration

**Reference**: Gemini Question 1 response from review phase

**Status**: ⏸️ Partially complete, 2 remaining fixes need architectural work

---

### Task 3: instanceof Narrowing Fix ✅ COMPLETE
**Function**: `narrow_by_instanceof`
**Bugs**: 1 (interface vs class)

**Implementation** (commit c884dc200):
- After `narrow_to_type`, if result is NEVER, create intersection
- This correctly handles interface vs class cases
- Preserves normal narrowing for assignable cases

**Status**: ✅ Complete - All tests pass

---

### Task 4: Regression Testing
**File**: `src/solver/tests/narrowing_regression_tests.rs`

**Test Cases**:
- Discriminant narrowing with shared values
- Optional properties in discriminants
- instanceof with interfaces vs classes
- `in` operator with unknown
- `in` operator with optional properties
- `in` operator with intersections
- All 8+ identified bug scenarios

**Status**: Not started

---

## Success Criteria

- [x] instanceof narrowing fixed (Task 3)
- [x] Unit tests pass with 100% coverage of edge cases
- [ ] in operator narrowing partially complete (Task 2)
- [ ] Discriminant narrowing fix (Task 1)
- [x] No regressions in existing narrowing tests
- [ ] Conformance tests match tsc exactly

---

## Complexity: HIGH

**Why High**:
- `src/solver/narrowing.rs` is high-traffic, high-impact
- Errors in union filtering → unsoundness or infinite recursion
- `Lazy` type resolution is tricky
- Must handle 25+ TypeKey variants correctly

**Implementation Principles**:
1. Use visitor pattern from `visitor.rs`
2. Always resolve `Lazy` types before inspection
3. Respect `strictNullChecks` setting
4. Follow Two-Question Rule (AGENTS.md)

---

## Next Step

**Task 1** (discriminant narrowing):
- Requires new Question 1 per Two-Question Rule
- Most complex task
- Must not repeat revert mistakes
- Uses filtering approach (already validated by Gemini)

**Task 2** (in operator fix):
- 2 of 4 fixes complete
- Remaining fixes need architectural changes
- Can be deferred or tackled in follow-up session

---

## Session History

- 2026-02-04: Completed review phase (8+ bugs found)
- 2026-02-04: Redefined as implementation session
- 2026-02-04: Task 2 partially complete (2 of 4 fixes, commit 68c367e2b)
- 2026-02-04: Task 3 complete (instanceof fix, commit c884dc200)
