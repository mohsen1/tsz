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

**Implementation**:

1. **Enhance `type_has_property`**:
   - Add `Lazy`/`Ref` resolution before shape lookup
   - Add `Intersection` support (true if ANY member has property)
   - Add prototype property checking
   - Keep index signature logic

2. **Fix `narrow_by_property_presence`**:
   - Transform from filter to **transformer**
   - For `unknown`: return `object & { prop: unknown }`
   - Add `promote_optional_property` helper

3. **New Helper: `promote_optional_property`**:
   - Clone ObjectShape
   - Set `optional: false` for the property
   - Re-intern shape

**Reference**: Gemini Question 1 response from review phase

**Status**: ⏸️ Plan ready, Question 1 already done

---

### Task 3: instanceof Narrowing Fix
**Function**: `narrow_by_instanceof`
**Bugs**: 1 (interface vs class)

**Implementation**:
- Use `interner.intersection2(source, target)` instead of `narrow_to_type`
- Handle structural vs nominal types correctly
- Respect prototype chain

**Status**: ⏸️ Plan ready, implementation pending

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

- [ ] All 3 narrowing functions fixed
- [ ] Unit tests pass with 100% coverage of edge cases
- [ ] Conformance tests match tsc exactly
- [ ] No "any poisoning" - unknown narrows correctly
- [ ] No regressions in existing narrowing tests

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

**Start with Task 2** (`in` operator fix):
- Question 1 already completed
- Clear implementation plan
- Will fix 4+ bugs at once

**Then Task 3** (instanceof fix):
- Simpler, quick win
- Uses similar patterns

**Then Task 1** (discriminant narrowing):
- Requires new Question 1
- Most complex
- Must not repeat revert mistakes

---

## Session History

- 2026-02-04: Completed review phase (8+ bugs found)
- 2026-02-04: Redefined as implementation session
- 2026-02-04: Ready to start Task 2 (in operator fix)
