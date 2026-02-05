# Session tsz-3: Advanced CFA Features

**Started**: 2026-02-05
**Status**: ✅ COMPLETE (2/4 phases delivered)
**Previous Session**: tsz-10 (CFA & Narrowing - Complete)

## Goal

Implement advanced Control Flow Analysis features to achieve 100% TypeScript parity.

## Progress Summary

**Delivered**: 2 major features (Phases 1 & 2)
**Blocked**: 2 features requiring deeper investigation (Phases 3 & 4)

---

## Phase 1: Bidirectional Narrowing (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED, TESTED, ALL TESTS PASS

**Problem**: Implement narrowing for `x === y` where both are references.

**Solution Implemented**:
1. **Flow Context Architecture**: Modified `narrow_type_by_condition` and related functions to accept `antecedent_id` parameter
2. **Bidirectional Narrowing Logic**: Enhanced `narrow_by_binary_expr` to get flow type of "other" reference
3. **Subtype Narrowing Fix**: Fixed `narrow_to_type` to handle subtype narrowing

**Test Cases Verified**:
```typescript
// Test 1: Basic bidirectional narrowing ✅
function test1(x: string | number, y: string) {
    if (x === y) { x.toLowerCase(); } // x correctly narrowed to string
}

// Test 2: Literal type narrowing ✅
function test4(x: string | number, y: string) {
    y = "hello";
    if (x === y) { x.toLowerCase(); } // x correctly narrowed to "hello"
}
```

**Commits**:
- `feat(cfa): implement bidirectional narrowing for reference equality`
- `feat(solver): fix subtype narrowing in narrow_to_type`

---

## Phase 2: Assertion Functions (✅ COMPLETE)

**Status**: ✅ IMPLEMENTED, TESTED, ALL TESTS PASS

**Problem**: Integration of `asserts x is T` with flow analysis for all subsequent code.

**Solution Implemented**:
1. **CALL nodes as merge points**: Modified `check_flow` to treat CALL nodes as merge points
2. **Dependency tracking**: CALL nodes wait for antecedents before processing
3. **Existing logic**: `handle_call_iterative` already handled assertions correctly

**Test Cases Verified**:
```typescript
function assertIsString(x: unknown): asserts x is string {
    if (typeof x !== "string") throw new Error();
}

// Test 1: Basic assertion ✅
function test1(x: unknown) {
    assertIsString(x);
    x.toLowerCase(); // x correctly narrowed to string
}

// Test 2: Error on incompatible assertion ✅
function test2(x: unknown) {
    assertIsString(x);
    assertIsNumber(x); // Error: x is never
}
```

**Commits**:
- `feat(cfa): implement assertion functions integration`

---

## Phase 3: Nested Discriminants (⏸️ BLOCKED)

**Status**: ⏸️ BLOCKED - BROKE 4 EXISTING TESTS

**Problem**: Support narrowing for nested discriminant paths like `action.payload.kind`.

**What Was Attempted**:
1. ✅ Implemented path collection: `discriminant_property_info` returns `Vec<Atom>`
2. ✅ Updated `resolve_type` to handle Application types
3. ❌ Attempted to remove `is_matching_reference` check
4. ⚠️ **Result**: Broke 4 existing tests
5. ⚠️ **Action**: Reverted to commit `d46a7450d`

**Root Cause** (per Gemini Pro):
The check `if self.is_matching_reference(base, target)` prevents nested narrowing:
- For `action.payload.kind === 'item'`: `base` is `action`, `target` is `action.payload.kind`
- They are NOT the same reference, so discriminant guard is not created
- Simply removing the check breaks other narrowing cases

**Requires**: AccessPath/FlowContainer abstraction similar to TypeScript's implementation
**Complexity**: Very High - multi-file architectural change
**Impact**: High - prevents real-world Redux/Flux pattern support

---

## Phase 4: Edge Cases (⏸️ BLOCKED)

**Status**: ⏸️ BLOCKED - BROKE 5 EXISTING TESTS

**Sub-phases**:
- 4.1: Narrowing `any` (ATTEMPTED - FAILED)
- 4.2: Truthiness of `0` and `""` (NOT STARTED)
- 4.3: Object freshness (NOT STARTED)

**What Was Attempted (4.1)**:
- Modified `narrow_by_typeof` to treat `ANY` same as `UNKNOWN`
- Removed early return for `TypeId::ANY`
- **Result**: Broke 5 circular extends tests
- **Action**: Reverted to commit `d46a7450d`

**Root Cause**:
The change triggered circularity errors in `src/solver/subtype.rs` or `src/solver/evaluate.rs`, likely due to:
- Missing recursion guards
- Improper handling of Lazy/Ref/Intersection types during narrowing
- Type system invariants being violated

**Requires**: Deep investigation into circular extends errors before proceeding
**Complexity**: High - requires understanding type resolution cycles

---

## Session Outcome

### Successfully Delivered ✅
1. **Bidirectional Narrowing** - Matches TypeScript exactly
2. **Assertion Functions** - Matches TypeScript exactly

Both features are significant improvements to tsz's CFA capabilities and pass all existing tests.

### Blocked Features ⏸️
1. **Nested Discriminants** - Requires architectural abstraction
2. **Any Narrowing** - Requires debugging circular extends

Both blocked features broke existing tests when attempted, indicating they touch deep type system invariants.

### Recommendation
This session has successfully delivered 2 major features. The blocked features (Phase 3 & 4.1) should be addressed in future sessions with:
1. Pre-implementation investigation into why tests would break
2. Careful analysis of type system invariants
3. Potential architectural refactoring to support nested discriminants

---

## Context from tsz-10

Session tsz-10 completed:
- ✅ Type guards (typeof, instanceof, discriminants, truthiness)
- ✅ Property access & assignment narrowing
- ✅ Exhaustiveness checking (fixed discriminant comparison bug)

See `docs/sessions/history/tsz-10.md` for details.

---

## Technical Notes

### Key Insights
1. **Two-Question Rule Works**: Both Phase 1 and Phase 2 followed the mandatory Gemini consultation workflow and succeeded
2. **Solver is Fragile**: Changes to narrowing logic can break distant parts of the type system (circular extends)
3. **Test Coverage is Critical**: The comprehensive test suite caught regressions that manual testing missed

### Files Modified (Successful Changes)
- `src/checker/control_flow.rs`: Flow context, CALL node handling
- `src/solver/narrowing.rs`: Subtype narrowing, Application type resolution

### Files Attempted (Reverted)
- `src/checker/control_flow_narrowing.rs`: Path collection for nested discriminants
- `src/solver/narrowing.rs`: Any narrowing (broke circular extends tests)

---

## Session Notes

This session continued the CFA work from tsz-10. Successfully delivered two complex features that bring tsz closer to TypeScript parity. The blocked features highlight areas where the type system needs architectural strengthening before advanced features can be safely added.
