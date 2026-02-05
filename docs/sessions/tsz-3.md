# Session tsz-3: CFA Refinement & Advanced Features

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE
**Previous Session**: tsz-3 Phase 1 (CFA Features - Complete)

## Goal

Implement advanced CFA features and unblock architectural issues to achieve 100% TypeScript parity.

## Context from Completed Session

Previous tsz-3 Phase 1 successfully delivered:
- ✅ Phase 1: Bidirectional Narrowing (x === y where both are references)
- ✅ Phase 2: Assertion Functions (asserts x is T)

**Session tsz-12 is now SUPERSEDED by this session** - all unique content has been merged.

---

## Phase 1: Nested Discriminants (🔄 ACTIVE - INVESTIGATION PAUSED)

**Status**: 🟡 IN PROGRESS - INVESTIGATION PAUSED DUE TO PRE-EXISTING TEST FAILURES

**Problem**: Support narrowing for nested discriminant paths like `action.payload.kind`.

**TypeScript Behavior**:
```typescript
type Action =
    | { type: 'UPDATE', payload: { kind: 'user', data: User } }
    | { type: 'UPDATE', payload: { kind: 'product', data: Product } };

function reducer(action: Action) {
    switch (action.payload.kind) {
        case 'user':
            return action.payload.data.name; // action.payload narrowed correctly
        case 'product':
            return action.payload.data.price;
    }
}
```

**Implementation Attempt (2026-02-05)**:

Modified `discriminant_property_info` to track relative paths:
- Added `relative_path_info` tracking for intermediate narrowing targets
- Check `is_matching_reference(current, target)` BEFORE adding segment to path
- Returns 4-tuple with relative info: `Option<(Vec<Atom>, bool, NodeIndex, Option<(Vec<Atom>, bool, NodeIndex)>)>`

Modified `discriminant_comparison`:
- Prioritizes relative_info for nested narrowing
- Falls back to base narrowing for root-level narrowing
- Returns `None` when `rel_path.is_empty()` (target is leaf)

**Issue Encountered**:
- Commit broke 4 pre-existing tests (unrelated to nested discriminants)
- Tests were already failing before the changes
- Reverted commit (f4cbae3c8) to avoid compounding issues

**Root Cause Analysis Needed**:
The following tests are failing (pre-existing, NOT caused by nested discriminant work):
1. `test_asserts_type_predicate_narrows_true_branch` - Expects `TypeId(9)` but gets `TypeId(130)`
2. `test_truthiness_false_branch_narrows_to_falsy`
3. `test_array_destructuring_assignment_clears_narrowing`
4. `test_array_destructuring_default_initializer_clears_narrowing`

**Next Steps**:
1. ⚠️ **BLOCKER**: Fix pre-existing test failures before continuing nested discriminant work
2. Ask Gemini to investigate the test failures
3. Once tests pass, re-implement nested discriminant narrowing
4. Investigate checker flow to ensure narrowing is requested for intermediate properties

---

## Phase 2: Assertion Functions (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED & TESTED

**Merged from tsz-12** - Assertion functions integration already completed in previous tsz-3.

---

## Phase 3: Edge Case Fixes (⏸️ PENDING)

### 3.1: Zombie Freshness
**Issue**: Fresh object literals might lose freshness after narrowing.

**Investigation**: Check `narrow_by_discriminant` in `src/solver/narrowing.rs`. Ensure narrowed types preserve `ObjectFlags::FRESH_LITERAL`.

### 3.2: Truthiness of 0 and ""
**Issue**: `if (x)` where `x: string | number` should narrow to `string (excluding "") | number (excluding 0)`.

**Investigation**: Verify `narrow_by_truthiness` handles `0` and `""` correctly.

### 3.3: Narrowing `any`
**Issue**: `typeof x === "string"` where `x: any` should narrow to `string` within the block.

**Investigation**: Check `narrow_by_typeof` in `src/solver/narrowing.rs`. Currently returns `ANY` immediately (line ~687). Should narrow to the specific type.

**Previous Attempt**: Broke 5 circular extends tests when trying to narrow `any`
**Requires**: Investigation into circular extends errors before retrying

---

## Session Notes

This session combines the goals of tsz-12 with the architectural investigation needed to unblock Phase 1 of the new tsz-3.

**Key Principles**:
- Follow Two-Question Rule strictly for ALL solver/checker changes
- Pre-implementation investigation to avoid breaking existing tests
- Architectural understanding before code changes

**Files Modified in Previous Session**:
- `src/checker/control_flow.rs`: Flow context, CALL node handling
- `src/solver/narrowing.rs`: Subtype narrowing, Application type resolution
