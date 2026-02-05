# Session tsz-3: CFA Refinement & Advanced Features

**Started**: 2026-02-05
**Status**: 🟡 ACTIVE - FIXING PRE-EXISTING REGRESSIONS
**Previous Session**: tsz-3 Phase 1 (CFA Features - Complete)

## Goal

Fix core CFA regressions to provide a stable foundation for advanced features.

## Priority Shift (2026-02-05)

**Gemini Recommendation**: Fix pre-existing test failures BEFORE implementing nested discriminants.

**Rationale**:
- The 4 failing tests indicate regressions in the core CFA foundation
- Likely caused by recent discriminant narrowing work (commit f2d4ae5d5)
- Building nested discriminants on a broken narrowing engine = "Zombie Narrowing"
- Must fix foundation before adding new features

---

## Phase 0: Fix CFA Regressions (🔄 ACTIVE - HIGH PRIORITY)

**Status**: 🟡 IN PROGRESS

**Failing Tests**:
1. `test_asserts_type_predicate_narrows_true_branch` - Gets `TypeId(130)` instead of `TypeId(9)` (union)
2. `test_truthiness_false_branch_narrows_to_falsy`
3. `test_array_destructuring_assignment_clears_narrowing`
4. `test_array_destructuring_default_initializer_clears_narrowing`

**Investigation Plan** (per Gemini guidance):

### Task 1: Fix Assertion and Truthiness Regressions
- **File**: `src/solver/narrowing.rs`
- **Focus**: `narrow_by_truthiness` and assertion-related functions
- **Hypothesis**: TypeId(130) is likely a `Ref`/`Application`/`Intersection` that isn't being resolved
- **Action**: Use `TSZ_LOG=debug TSZ_LOG_FORMAT=tree cargo test test_asserts_type_predicate_narrows_true_branch`
- **Fix**: Ensure all types pass through `resolve_type()` or `evaluate()` before narrowing operations

### Task 2: Fix Array Destructuring Narrowing
- **File**: `src/checker/control_flow.rs`
- **Focus**: `handle_assignment` and `clear_narrowing_for_reference`
- **Problem**: Destructuring assignments (`[x] = arr`) should invalidate narrowing for `x`
- **Action**: Ensure binder/checker identifies mutations in destructuring patterns

### Task 3: Re-verify All Tests Pass
- **Command**: `cargo nextest run` (or `cargo test`)
- **Goal**: 100% test pass rate before proceeding

**Estimated Effort**: 4-6 hours (deep solver tracing required)

## Context from Completed Session

Previous tsz-3 Phase 1 successfully delivered:
- ✅ Phase 1: Bidirectional Narrowing (x === y where both are references)
- ✅ Phase 2: Assertion Functions (asserts x is T)

**Session tsz-12 is now SUPERSEDED by this session** - all unique content has been merged.

---

## Phase 1: Fix CFA Regressions (🔄 ACTIVE - COMPLEX INTERACTION)

**Status**: 🟡 IN PROVESTIGATION - FOUND FIX BUT BREAKS OTHER TESTS

**Test Failures**:

### Fixed (but reverted due to circular extends failures):
1. ✅ `test_asserts_type_predicate_narrows_true_branch` - **FIXED** but reverted

### Still Failing:
2. ❌ `test_truthiness_false_branch_narrows_to_falsy`
3. ❌ `test_array_destructuring_assignment_clears_narrowing`
4. ❌ `test_array_destructuring_default_initializer_clears_narrowing`

### Circular Extends Tests (BLOCKER):
5. ❌ `test_circular_extends_chain_with_endpoint_bound`
6. ❌ `test_circular_extends_conflicting_lower_bounds`
7. ❌ `test_circular_extends_with_literal_types`
8. ❌ `test_circular_extends_with_concrete_upper_and_lower`
9. ❌ `test_circular_extends_three_way_with_one_lower_bound`

**Root Cause Identified** (2026-02-05):

The assertion test failure was in `src/solver/narrowing.rs:1784`:
```rust
TypeGuard::Predicate { type_id, asserts } => {
    match type_id {
        Some(target_type) => {
            if sense {
                self.narrow_to_type(source_type, *target_type)
            } else {
                self.narrow_excluding_type(source_type, *target_type)  // ❌ BUG
            }
        }
    }
}
```

**Problem**: The code doesn't check the `asserts` flag before narrowing in the false branch.

**Fix Applied** (commit c25830407 - REVERTED):
```rust
TypeGuard::Predicate { type_id, asserts } => {
    // CRITICAL: asserts predicates only narrow in the true branch
    if *asserts && !sense {
        return source_type;  // Don't narrow in false branch for assertions
    }

    match type_id {
        Some(target_type) => {
            if sense {
                self.narrow_to_type(source_type, *target_type)
            } else {
                self.narrow_excluding_type(source_type, *target_type)
            }
        }
    }
}
```

**Issue**: The fix broke 5 circular extends tests, suggesting a complex interaction between predicate narrowing and type resolution.

**Next Steps**:
1. ⚠️ **CRITICAL**: Need to understand why the fix breaks circular extends
2. Ask Gemini: "Why does checking `asserts && !sense` break circular extends?"
3. May need to fix circular extends tests first, or find alternative approach
4. Risk: These tests may have been passing for the wrong reasons

---

## Phase 2: Nested Discriminants (⏸️ PAUSED - BLOCKED BY PHASE 1)

**Status**: ⏸️ PAUSED - BLOCKED BY PHASE 1

**Implementation Status**:
- ✅ Code written and reviewed by Gemini
- ✅ Architecture validated
- ⏸️ Awaiting test suite stability

**What's Ready** (commit 9add349ee - REVERTED):
- `discriminant_property_info`: Tracks relative paths for intermediate narrowing targets
- `discriminant_comparison`: Prioritizes relative_info over base narrowing
- `discriminant_property`: Uses relative paths when available

**Next Steps** (after Phase 1 complete):
1. Re-apply the nested discriminant changes
2. Test with `tests/nested_discriminant.test.ts`
3. Investigate checker flow to ensure narrowing is requested for intermediate properties

**Estimated Complexity**: MEDIUM (2-3 hours once foundation is stable)

---

## Phase 3: Edge Case Fixes (⏸️ PENDING - BLOCKED BY PHASE 1)

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
